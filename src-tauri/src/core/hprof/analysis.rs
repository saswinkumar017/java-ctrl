use crate::core::hprof::model::{
    primitive_array_name, short_id, ObjKind, ObjectRecord, Pass1, MAX_DOMINATOR_OBJECTS,
};
use crate::core::models::{
    HeapAnalysis, HeapClassRow, HeapLoaderRow, HeapObjectRow, RootCountRow,
};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::time::Instant;

/// Turn a completed two-pass parse into the payload consumed by the Heap
/// Explorer. Reachability is computed from GC roots; objects reachable only
/// through `Reference.referent` are reported separately so that caches and
/// weak maps do not inflate the "live" numbers.
pub fn analyze(
    state: Pass1,
    id_size: u8,
    timestamp: i64,
    file_size: u64,
    path: String,
    mismatch: usize,
    started: Instant,
) -> HeapAnalysis {
    let total_objects = state.objects.len() as u64;
    let total_shallow_bytes = state.objects.iter().map(|object| object.shallow).sum();

    let roots = collect_roots(&state);
    let reachable = mark_reachable(&state, &roots);
    let weak = mark_weak(&state, &reachable, &state.weak_targets);

    let reachable_objects = reachable.iter().filter(|&&value| value != 0).count() as u64;
    let reachable_bytes = state
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| reachable[*index] != 0)
        .map(|(_, object)| object.shallow)
        .sum();
    let weak_objects = weak.iter().filter(|&&value| value != 0).count() as u64;
    let weak_bytes = state
        .objects
        .iter()
        .enumerate()
        .filter(|(index, _)| weak[*index] != 0)
        .map(|(_, object)| object.shallow)
        .sum();

    let class_names: Vec<&str> = state
        .classes
        .iter()
        .map(|class| class.name.as_str())
        .collect();

    let mut class_aggs: HashMap<&str, (u64, u64)> = HashMap::new();
    let mut weak_class_aggs: HashMap<&str, (u64, u64)> = HashMap::new();
    let mut loader_aggs: HashMap<Option<u64>, LoaderAgg> = HashMap::new();

    for (index, object) in state.objects.iter().enumerate() {
        let name = object_name(&state, &class_names, object);
        let entry = class_aggs.entry(name).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += object.shallow;

        if weak[index] != 0 {
            let entry = weak_class_aggs.entry(name).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += object.shallow;
        }

        let loader = loader_of(&state, object);
        let aggregate = loader_aggs.entry(loader).or_default();
        aggregate.objects += 1;
        aggregate.bytes += object.shallow;
    }

    for class in &state.classes {
        loader_aggs
            .entry(Some(class.loader_id))
            .or_default()
            .classes += 1;
    }

    let classes = class_rows(class_aggs, 200);
    let weak_classes = class_rows(weak_class_aggs, 100);

    let mut loaders: Vec<HeapLoaderRow> = loader_aggs
        .into_iter()
        .map(|(loader, aggregate)| HeapLoaderRow {
            loader: state.loader_label(loader),
            objects: aggregate.objects,
            bytes: aggregate.bytes,
            classes: aggregate.classes,
        })
        .collect();
    loaders.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.loader.cmp(&b.loader))
    });
    loaders.truncate(50);

    let mut top_heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    for (index, object) in state.objects.iter().enumerate() {
        push_top(&mut top_heap, 25, object.shallow, index as u32);
    }
    let mut top_indices: Vec<(u64, u32)> = top_heap
        .into_iter()
        .map(|Reverse((shallow, index))| (shallow, index))
        .collect();
    top_indices.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let dominators = compute_dominators(&state, &reachable, &roots, MAX_DOMINATOR_OBJECTS);

    let top_objects: Vec<HeapObjectRow> = top_indices
        .iter()
        .map(|&(shallow, index)| {
            let object = &state.objects[index as usize];
            HeapObjectRow {
                object_id: short_id(object.id),
                class_name: object_name(&state, &class_names, object).to_string(),
                shallow_bytes: shallow,
                retained_bytes: dominators
                    .as_ref()
                    .map(|result| result.retained_by_object[index as usize]),
            }
        })
        .collect();

    let (dominator_rows, dominators_computed) = match &dominators {
        Some(result) => {
            let rows: Vec<HeapObjectRow> = result
                .top
                .iter()
                .map(|&(index, retained)| {
                    let object = &state.objects[index as usize];
                    HeapObjectRow {
                        object_id: short_id(object.id),
                        class_name: object_name(&state, &class_names, object).to_string(),
                        shallow_bytes: object.shallow,
                        retained_bytes: Some(retained),
                    }
                })
                .collect();
            (rows, true)
        }
        None => (Vec::new(), false),
    };

    let mut root_counts: Vec<RootCountRow> = state
        .root_counts
        .iter()
        .map(|(kind, count)| RootCountRow {
            kind: kind.clone(),
            count: *count,
        })
        .collect();
    root_counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.kind.cmp(&b.kind)));
    root_counts.truncate(50);

    let mut notes = vec![format!(
        "Parsed {} objects and {} classes from a {} dump with {}-byte identifiers.",
        total_objects,
        state.classes.len(),
        state.version,
        id_size
    )];
    if mismatch > 0 {
        notes.push(format!(
            "{mismatch} instance record(s) did not match their class layout; outgoing references from those objects were not followed."
        ));
    }
    if !dominators_computed {
        notes.push(format!(
            "Retained-size analysis was skipped because the dump has more than {MAX_DOMINATOR_OBJECTS} reachable objects."
        ));
    }
    if weak_objects > 0 {
        notes.push(
            "Weakly reachable objects are counted separately from live objects; they are reachable only through Reference.referent.".to_string(),
        );
    }
    if state
        .root_counts
        .get("Unreachable")
        .copied()
        .unwrap_or(0)
        > 0
    {
        notes.push(
            "The dump contains explicit unreachable roots; they are excluded from the live-object calculation."
                .to_string(),
        );
    }
    if classes.len() >= 200 {
        notes.push("Class table is capped to the 200 largest entries.".to_string());
    }

    HeapAnalysis {
        path,
        file_size,
        dump_timestamp: if timestamp > 0 { Some(timestamp) } else { None },
        id_size,
        total_objects,
        total_shallow_bytes,
        class_count: state.classes.len() as u64,
        reachable_objects,
        reachable_bytes,
        weak_objects,
        weak_bytes,
        classes,
        weak_classes,
        loaders,
        top_objects,
        dominators: dominator_rows,
        root_counts,
        analysis_ms: started.elapsed().as_millis() as u64,
        dominators_computed,
        notes,
    }
}

#[derive(Default)]
struct LoaderAgg {
    objects: u64,
    bytes: u64,
    classes: u64,
}

struct DominatorResult {
    retained_by_object: Vec<u64>,
    top: Vec<(u32, u64)>,
}

fn collect_roots(state: &Pass1) -> Vec<u32> {
    let mut roots = Vec::new();
    for (id, _) in &state.roots {
        if let Some(&index) = state.object_index.get(id) {
            roots.push(index);
        }
    }
    roots.sort_unstable();
    roots.dedup();
    roots
}

fn mark_reachable(state: &Pass1, roots: &[u32]) -> Vec<u8> {
    let mut reachable = vec![0u8; state.objects.len()];
    let mut stack = Vec::new();
    for &root in roots {
        if reachable[root as usize] == 0 {
            reachable[root as usize] = 1;
            stack.push(root);
        }
    }
    while let Some(index) = stack.pop() {
        let object = &state.objects[index as usize];
        let start = object.refs_start as usize;
        let end = start + object.refs_len as usize;
        for &target in &state.edges[start..end] {
            if reachable[target as usize] == 0 {
                reachable[target as usize] = 1;
                stack.push(target);
            }
        }
    }
    reachable
}

/// Weakly reachable objects are found by walking normal references outward
/// from every weak referent. Anything already strongly reachable is skipped:
/// a weak reference to a live object has no effect on liveness.
fn mark_weak(state: &Pass1, reachable: &[u8], weak_targets: &[u32]) -> Vec<u8> {
    let mut weak = vec![0u8; state.objects.len()];
    let mut stack = Vec::new();
    for &target in weak_targets {
        let index = target as usize;
        if reachable[index] == 0 && weak[index] == 0 {
            weak[index] = 1;
            stack.push(target);
        }
    }
    while let Some(index) = stack.pop() {
        let object = &state.objects[index as usize];
        let start = object.refs_start as usize;
        let end = start + object.refs_len as usize;
        for &target in &state.edges[start..end] {
            let index = target as usize;
            if reachable[index] == 0 && weak[index] == 0 {
                weak[index] = 1;
                stack.push(target);
            }
        }
    }
    weak
}

fn object_name<'a>(
    _state: &'a Pass1,
    class_names: &'a [&'a str],
    object: &ObjectRecord,
) -> &'a str {
    if object.class_object {
        return "java.lang.Class";
    }
    if let Some(name) = class_names.get(object.class_idx as usize) {
        return *name;
    }
    match object.kind {
        ObjKind::PrimitiveArray(ty, _) => primitive_array_name(ty),
        ObjKind::ObjectArray(_) => "[Ljava.lang.Object;",
        _ => "[unknown]",
    }
}

fn loader_of(state: &Pass1, object: &ObjectRecord) -> Option<u64> {
    if object.class_object {
        return Some(0);
    }
    if let Some(class) = state.classes.get(object.class_idx as usize) {
        return Some(class.loader_id);
    }
    if matches!(object.kind, ObjKind::PrimitiveArray(..)) {
        return Some(0);
    }
    None
}

fn class_rows(map: HashMap<&str, (u64, u64)>, limit: usize) -> Vec<HeapClassRow> {
    let mut rows: Vec<HeapClassRow> = map
        .into_iter()
        .map(|(class_name, (instances, bytes))| HeapClassRow {
            class_name: class_name.to_string(),
            instances,
            bytes,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.class_name.cmp(&b.class_name))
    });
    rows.truncate(limit);
    rows
}

fn push_top(heap: &mut BinaryHeap<Reverse<(u64, u32)>>, limit: usize, value: u64, index: u32) {
    if heap.len() < limit {
        heap.push(Reverse((value, index)));
        return;
    }
    if let Some(&Reverse((smallest, _))) = heap.peek() {
        if value > smallest {
            heap.pop();
            heap.push(Reverse((value, index)));
        }
    }
}

/// Lengauer-Tarjan dominators over the live-object graph. Retained size is
/// the sum of shallow sizes in an object's dominator subtree, which is the
/// standard definition used by heap analysers.
fn compute_dominators(
    state: &Pass1,
    reachable: &[u8],
    roots: &[u32],
    limit: usize,
) -> Option<DominatorResult> {
    let reachable_count = reachable.iter().filter(|&&value| value != 0).count();
    if reachable_count == 0 || reachable_count > limit {
        return None;
    }

    let object_count = state.objects.len();
    let mut dfs_of_object = vec![u32::MAX; object_count];
    let mut vertex: Vec<u32> = Vec::with_capacity(reachable_count + 1);
    let mut parent: Vec<u32> = Vec::with_capacity(reachable_count + 1);
    vertex.push(u32::MAX); // virtual root at DFS index 0
    parent.push(0);

    let mut stack: Vec<(u32, u32)> = Vec::new();
    for &root in roots.iter().rev() {
        if reachable[root as usize] == 0 || dfs_of_object[root as usize] != u32::MAX {
            continue;
        }
        let dense = vertex.len() as u32;
        dfs_of_object[root as usize] = dense;
        vertex.push(root);
        parent.push(0);
        stack.push((root, 0));

        // Finish this root's subtree before starting the next one. Dominator
        // numbering is only valid when DFS subtrees are contiguous.
        while let Some(top) = stack.last_mut() {
            let index = top.0;
            let object = &state.objects[index as usize];
            let start = object.refs_start;
            let end = start + object.refs_len;
            if start + top.1 < end {
                let target = state.edges[(start + top.1) as usize];
                top.1 += 1;
                if reachable[target as usize] != 0 && dfs_of_object[target as usize] == u32::MAX {
                    let dense = vertex.len() as u32;
                    dfs_of_object[target as usize] = dense;
                    vertex.push(target);
                    parent.push(dfs_of_object[index as usize]);
                    stack.push((target, 0));
                }
            } else {
                stack.pop();
            }
        }
    }

    if vertex.len() - 1 != reachable_count {
        // The DFS and the reachability walk disagree; refuse to emit a
        // retained-size table rather than reporting wrong dominators.
        return None;
    }

    let count = reachable_count;
    let mut pred_count = vec![0u32; count + 1];
    for dense in 1..=count {
        let object = &state.objects[vertex[dense] as usize];
        let start = object.refs_start as usize;
        let end = start + object.refs_len as usize;
        for &target in &state.edges[start..end] {
            let target_dense = dfs_of_object[target as usize];
            if target_dense != u32::MAX {
                pred_count[target_dense as usize] += 1;
            }
        }
    }

    let mut pred_off = vec![0usize; count + 2];
    for dense in 1..=count {
        pred_off[dense + 1] = pred_off[dense] + pred_count[dense] as usize;
    }
    let mut cursor = pred_off.clone();
    let mut preds = vec![0u32; pred_off[count + 1]];
    for dense in 1..=count {
        let object = &state.objects[vertex[dense] as usize];
        let start = object.refs_start as usize;
        let end = start + object.refs_len as usize;
        for &target in &state.edges[start..end] {
            let target_dense = dfs_of_object[target as usize];
            if target_dense != u32::MAX {
                let slot = cursor[target_dense as usize];
                preds[slot] = dense as u32;
                cursor[target_dense as usize] = slot + 1;
            }
        }
    }

    let mut semi: Vec<u32> = (0..=count as u32).collect();
    let mut label = semi.clone();
    let mut ancestor = vec![0u32; count + 1];
    let mut idom = vec![0u32; count + 1];
    let mut bucket_head = vec![u32::MAX; count + 1];
    let mut bucket_next = vec![u32::MAX; count + 1];
    let mut scratch: Vec<u32> = Vec::new();

    for w in (1..=count).rev() {
        let mut semi_w = semi[w];
        for &v in &preds[pred_off[w]..pred_off[w + 1]] {
            let u = eval(v, &mut ancestor, &mut label, &semi, &mut scratch);
            if semi[u as usize] < semi_w {
                semi_w = semi[u as usize];
            }
        }
        semi[w] = semi_w;

        let bucket = semi_w as usize;
        bucket_next[w] = bucket_head[bucket];
        bucket_head[bucket] = w as u32;

        let p = parent[w];
        ancestor[w] = p;
        let mut v = bucket_head[p as usize];
        bucket_head[p as usize] = u32::MAX;
        while v != u32::MAX {
            let next = bucket_next[v as usize];
            let u = eval(v, &mut ancestor, &mut label, &semi, &mut scratch);
            idom[v as usize] = if semi[u as usize] < semi[v as usize] {
                u
            } else {
                p
            };
            v = next;
        }
    }

    for w in 1..=count {
        if idom[w] != semi[w] {
            idom[w] = idom[idom[w] as usize];
        }
    }

    let mut retained_dfs = vec![0u64; count + 1];
    for dense in 1..=count {
        retained_dfs[dense] = state.objects[vertex[dense] as usize].shallow;
    }
    for dense in (1..=count).rev() {
        let dominator = idom[dense] as usize;
        retained_dfs[dominator] = retained_dfs[dominator].saturating_add(retained_dfs[dense]);
    }

    let mut retained_by_object = vec![0u64; object_count];
    for dense in 1..=count {
        retained_by_object[vertex[dense] as usize] = retained_dfs[dense];
    }

    let mut top_heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    for dense in 1..=count {
        push_top(&mut top_heap, 50, retained_dfs[dense], vertex[dense]);
    }
    let mut top: Vec<(u32, u64)> = top_heap
        .into_iter()
        .map(|Reverse((retained, index))| (index, retained))
        .collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    Some(DominatorResult {
        retained_by_object,
        top,
    })
}

/// Iterative path-compressing `eval` from Lengauer-Tarjan. `scratch` is reused
/// between calls to keep the hot loop allocation-free.
fn eval(
    v: u32,
    ancestor: &mut [u32],
    label: &mut [u32],
    semi: &[u32],
    scratch: &mut Vec<u32>,
) -> u32 {
    if ancestor[v as usize] == 0 {
        return label[v as usize];
    }
    scratch.clear();
    let mut u = v;
    while ancestor[u as usize] != 0 && ancestor[ancestor[u as usize] as usize] != 0 {
        scratch.push(u);
        u = ancestor[u as usize];
    }
    for &node in scratch.iter().rev() {
        let parent = ancestor[node as usize] as usize;
        if semi[label[parent] as usize] < semi[label[node as usize] as usize] {
            label[node as usize] = label[parent];
        }
        ancestor[node as usize] = ancestor[parent];
    }
    label[v as usize]
}
