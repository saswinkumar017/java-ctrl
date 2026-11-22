use crate::core::error::{JccError, Result};
use crate::core::hprof::model::{
    header_size, ClassMeta, FieldDef, FieldType, ObjKind, Pass1,
};
use crate::core::hprof::reader::{
    read_class_dump, read_header, read_root_body, ClassDump, HprofReader,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub fn read_pass1(path: &Path) -> Result<(Pass1, u8, i64, u64)> {
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut buffered = BufReader::with_capacity(1024 * 1024, file);
    let (version, id_size, timestamp) = read_header(&mut buffered)
        .map_err(|err| JccError::parse(format!("{} is not a valid HPROF dump: {err}", path.display())))?;
    let mut state = Pass1 {
        version,
        ..Default::default()
    };
    let mut reader = HprofReader::new(buffered, id_size as usize);

    while let Some(tag) = reader.next_record().map_err(parse_err)? {
        match tag {
            0x01 => {
                let id = reader.read_id().map_err(parse_err)?;
                let value = reader.read_string_body().map_err(parse_err)?;
                state.strings.insert(id, value);
            }
            0x02 => {
                let _serial = reader.read_u32().map_err(parse_err)?;
                let class_id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let name_id = reader.read_id().map_err(parse_err)?;
                state.class_names.insert(class_id, name_id);
                state.ensure_class(class_id, name_id);
            }
            0x0C | 0x1C => process_heap_pass1(&mut reader, &mut state, id_size as usize)?,
            0x2C => {}
            _ => {}
        }
        reader.skip_remaining().map_err(parse_err)?;
    }

    finalize_pass1(&mut state)?;
    Ok((state, id_size, timestamp, file_size))
}

pub fn read_pass2(path: &Path, state: &mut Pass1) -> Result<usize> {
    let file = File::open(path)?;
    let mut buffered = BufReader::with_capacity(1024 * 1024, file);
    let (_version, id_size, _timestamp) = read_header(&mut buffered)
        .map_err(|err| JccError::parse(format!("{} is not a valid HPROF dump: {err}", path.display())))?;
    let mut reader = HprofReader::new(buffered, id_size as usize);
    let mut objects = std::mem::take(&mut state.objects);
    let mut edges = std::mem::take(&mut state.edges);
    let mut weak_targets = std::mem::take(&mut state.weak_targets);
    let mut mismatch = 0usize;

    while let Some(tag) = reader.next_record().map_err(parse_err)? {
        if tag == 0x0C || tag == 0x1C {
            process_heap_pass2(
                &mut reader,
                &mut objects,
                &state.object_index,
                &state.layouts,
                &mut edges,
                &mut weak_targets,
                &mut mismatch,
            )?;
        }
        reader.skip_remaining().map_err(parse_err)?;
    }

    state.objects = objects;
    state.edges = edges;
    state.weak_targets = weak_targets;
    Ok(mismatch)
}

fn parse_err(err: std::io::Error) -> JccError {
    JccError::parse(format!("HPROF parse error: {err}"))
}

fn process_heap_pass1<R: Read>(
    reader: &mut HprofReader<R>,
    state: &mut Pass1,
    id_size: usize,
) -> Result<()> {
    while reader.remaining() > 0 {
        let tag = reader.read_u8().map_err(parse_err)?;
        match tag {
            0x20 => {
                let dump = read_class_dump(reader).map_err(parse_err)?;
                apply_class_dump(state, dump, id_size);
            }
            0x21 => {
                let id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let class_id = reader.read_id().map_err(parse_err)?;
                let nbytes = reader.read_u32().map_err(parse_err)?;
                reader.skip(nbytes as u64).map_err(parse_err)?;
                state.add_object(
                    id,
                    class_id,
                    false,
                    header_size(id_size) + nbytes as u64,
                    ObjKind::Instance,
                );
            }
            0x22 => {
                let id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let count = reader.read_u32().map_err(parse_err)?;
                let class_id = reader.read_id().map_err(parse_err)?;
                reader
                    .skip(count as u64 * id_size as u64)
                    .map_err(parse_err)?;
                state.add_object(
                    id,
                    class_id,
                    false,
                    header_size(id_size) + count as u64 * id_size as u64,
                    ObjKind::ObjectArray(count),
                );
            }
            0x23 => {
                let id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let count = reader.read_u32().map_err(parse_err)?;
                let raw_type = reader.read_u8().map_err(parse_err)?;
                let ty = FieldType::from_u8(raw_type)
                    .filter(|ty| *ty != FieldType::Object)
                    .ok_or_else(|| {
                        JccError::parse(format!("invalid primitive array type {raw_type}"))
                    })?;
                let bytes = count as u64 * ty.size(id_size) as u64;
                reader.skip(bytes).map_err(parse_err)?;
                state.add_object(
                    id,
                    class_id_from_primitive_array(ty),
                    false,
                    header_size(id_size) + bytes,
                    ObjKind::PrimitiveArray(raw_type, count),
                );
            }
            0xFE => {
                let _heap = reader.read_u32().map_err(parse_err)?;
                let _name = reader.read_id().map_err(parse_err)?;
            }
            root if is_root_tag(root) => {
                let (id, kind) = read_root_body(reader, root).map_err(parse_err)?;
                *state.root_counts.entry(kind.to_string()).or_insert(0) += 1;
                if kind != "Unreachable" {
                    state.roots.push((id, kind));
                }
            }
            other => {
                return Err(JccError::parse(format!(
                    "unknown HPROF heap subrecord tag 0x{other:02x}"
                )));
            }
        }
    }
    Ok(())
}

fn is_root_tag(tag: u8) -> bool {
    matches!(tag, 0xFF | 0x01..=0x08 | 0x89..=0x8D | 0x90)
}

/// Primitive array dumps often omit a LOAD_CLASS record for their array class.
/// A synthetic id keeps the object record consistent; the display name is
/// derived from the primitive type when no class metadata exists.
fn class_id_from_primitive_array(ty: FieldType) -> u64 {
    let byte = match ty {
        FieldType::Boolean => 4u8,
        FieldType::Char => 5,
        FieldType::Float => 6,
        FieldType::Double => 7,
        FieldType::Byte => 8,
        FieldType::Short => 9,
        FieldType::Int => 10,
        FieldType::Long => 11,
        FieldType::Object => 0,
    };
    u64::MAX - byte as u64
}

fn apply_class_dump(state: &mut Pass1, dump: ClassDump, id_size: usize) {
    let name_id = state.class_names.get(&dump.class_id).copied().unwrap_or(0);
    let index = state.ensure_class(dump.class_id, name_id);
    let instance_fields = dump
        .instance_fields
        .into_iter()
        .map(|(name_id, ty)| FieldDef {
            name: state
                .strings
                .get(&name_id)
                .cloned()
                .unwrap_or_else(|| format!("field_{name_id:x}")),
            name_id,
            ty,
            weak: false,
        })
        .collect::<Vec<_>>();
    let static_refs = dump
        .static_fields
        .iter()
        .filter_map(|(_, _, value)| *value)
        .collect::<Vec<_>>();

    let class = &mut state.classes[index];
    class.super_id = dump.super_id;
    class.loader_id = dump.loader_id;
    class.instance_size = dump.instance_size;
    class.instance_fields = instance_fields;
    class.static_refs = static_refs;

    state.add_object(
        dump.class_id,
        0,
        true,
        header_size(id_size) + 32,
        ObjKind::ClassObject,
    );
}

fn finalize_pass1(state: &mut Pass1) -> Result<()> {
    if state.objects.is_empty() {
        return Err(JccError::parse(
            "the heap dump contains no object records; it may be truncated or unsupported",
        ));
    }

    for class in &mut state.classes {
        class.name = state
            .strings
            .get(&class.name_id)
            .cloned()
            .unwrap_or_else(|| format!("class 0x{:x}", class.class_id))
            .replace('/', ".");
    }

    state.layouts = build_layouts(&state.classes, &state.class_index);

    for object in &mut state.objects {
        if object.class_object {
            continue;
        }
        if let Some(&index) = state.class_index.get(&object.class_id) {
            object.class_idx = index as u32;
        }
    }

    let mut objects = std::mem::take(&mut state.objects);
    let mut edges = std::mem::take(&mut state.edges);
    for class in &state.classes {
        let Some(&object_index) = state.object_index.get(&class.class_id) else {
            continue;
        };
        let mut refs = Vec::new();
        for id in &class.static_refs {
            if let Some(&target) = state.object_index.get(id) {
                refs.push(target);
            }
        }
        if !refs.is_empty() {
            let object = &mut objects[object_index as usize];
            object.refs_start = edges.len() as u32;
            object.refs_len = refs.len() as u32;
            edges.extend(refs);
        }
    }
    state.objects = objects;
    state.edges = edges;
    Ok(())
}

fn build_layouts(
    classes: &[ClassMeta],
    class_index: &HashMap<u64, usize>,
) -> Vec<Vec<FieldDef>> {
    let mut cache: Vec<Option<Vec<FieldDef>>> = vec![None; classes.len()];
    let mut visiting = vec![false; classes.len()];
    for index in 0..classes.len() {
        build_layout(index, classes, class_index, &mut cache, &mut visiting);
    }
    cache.into_iter().map(|layout| layout.unwrap_or_default()).collect()
}

fn build_layout(
    index: usize,
    classes: &[ClassMeta],
    class_index: &HashMap<u64, usize>,
    cache: &mut [Option<Vec<FieldDef>>],
    visiting: &mut [bool],
) -> Vec<FieldDef> {
    if let Some(layout) = &cache[index] {
        return layout.clone();
    }
    if visiting[index] {
        return Vec::new();
    }
    visiting[index] = true;

    let mut layout = if classes[index].super_id != 0 {
        class_index
            .get(&classes[index].super_id)
            .map(|&super_index| build_layout(super_index, classes, class_index, cache, visiting))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let is_reference = hierarchy_contains_reference(index, classes, class_index);
    for field in &classes[index].instance_fields {
        let mut field = field.clone();
        if is_reference && field.name == "referent" {
            field.weak = true;
        }
        layout.push(field);
    }

    visiting[index] = false;
    cache[index] = Some(layout.clone());
    layout
}

fn hierarchy_contains_reference(
    mut index: usize,
    classes: &[ClassMeta],
    class_index: &HashMap<u64, usize>,
) -> bool {
    for _ in 0..64 {
        let class = &classes[index];
        if class.name == "java.lang.ref.Reference" || class.name.ends_with(".Reference") {
            return true;
        }
        if class.super_id == 0 {
            return false;
        }
        let Some(&super_index) = class_index.get(&class.super_id) else {
            return false;
        };
        index = super_index;
    }
    false
}

fn process_heap_pass2<R: Read>(
    reader: &mut HprofReader<R>,
    objects: &mut [crate::core::hprof::model::ObjectRecord],
    object_index: &HashMap<u64, u32>,
    layouts: &[Vec<FieldDef>],
    edges: &mut Vec<u32>,
    weak_targets: &mut Vec<u32>,
    mismatch: &mut usize,
) -> Result<()> {
    let id_size = reader.id_size();
    while reader.remaining() > 0 {
        let tag = reader.read_u8().map_err(parse_err)?;
        match tag {
            0x20 => {
                let _ = read_class_dump(reader).map_err(parse_err)?;
            }
            0x21 => {
                let id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let _class_id = reader.read_id().map_err(parse_err)?;
                let nbytes = reader.read_u32().map_err(parse_err)?;
                let Some(&slot) = object_index.get(&id) else {
                    reader.skip(nbytes as u64).map_err(parse_err)?;
                    continue;
                };
                let class_idx = objects[slot as usize].class_idx;
                let layout = if class_idx != u32::MAX {
                    layouts.get(class_idx as usize)
                } else {
                    None
                };
                let mut refs = Vec::new();
                let mut consumed = 0u64;
                let mut decoded = false;
                if let Some(layout) = layout {
                    let total = layout
                        .iter()
                        .map(|field| field.ty.size(id_size) as u64)
                        .sum::<u64>();
                    if total <= nbytes as u64 {
                        decoded = true;
                        for field in layout {
                            let size = field.ty.size(id_size) as u64;
                            if consumed + size > nbytes as u64 {
                                break;
                            }
                            if field.ty == FieldType::Object {
                                let target_id = reader.read_id().map_err(parse_err)?;
                                if let Some(&target) = object_index.get(&target_id) {
                                    if field.weak {
                                        weak_targets.push(target);
                                    } else {
                                        refs.push(target);
                                    }
                                }
                            } else {
                                reader.skip(size).map_err(parse_err)?;
                            }
                            consumed += size;
                        }
                    }
                }
                if !decoded && nbytes > 0 {
                    *mismatch += 1;
                }
                reader
                    .skip(nbytes as u64 - consumed)
                    .map_err(parse_err)?;
                let start = edges.len() as u32;
                let length = refs.len() as u32;
                edges.extend(refs);
                let object = &mut objects[slot as usize];
                object.refs_start = start;
                object.refs_len = length;
            }
            0x22 => {
                let id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let count = reader.read_u32().map_err(parse_err)?;
                let _class_id = reader.read_id().map_err(parse_err)?;
                let Some(&slot) = object_index.get(&id) else {
                    reader
                        .skip(count as u64 * id_size as u64)
                        .map_err(parse_err)?;
                    continue;
                };
                let mut refs = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let target_id = reader.read_id().map_err(parse_err)?;
                    if let Some(&target) = object_index.get(&target_id) {
                        refs.push(target);
                    }
                }
                let start = edges.len() as u32;
                let length = refs.len() as u32;
                edges.extend(refs);
                let object = &mut objects[slot as usize];
                object.refs_start = start;
                object.refs_len = length;
            }
            0x23 => {
                let _id = reader.read_id().map_err(parse_err)?;
                let _stack = reader.read_u32().map_err(parse_err)?;
                let count = reader.read_u32().map_err(parse_err)?;
                let raw_type = reader.read_u8().map_err(parse_err)?;
                let ty = FieldType::from_u8(raw_type)
                    .filter(|ty| *ty != FieldType::Object)
                    .ok_or_else(|| {
                        JccError::parse(format!("invalid primitive array type {raw_type}"))
                    })?;
                reader
                    .skip(count as u64 * ty.size(id_size) as u64)
                    .map_err(parse_err)?;
            }
            0xFE => {
                let _heap = reader.read_u32().map_err(parse_err)?;
                let _name = reader.read_id().map_err(parse_err)?;
            }
            root if is_root_tag(root) => {
                let _ = read_root_body(reader, root).map_err(parse_err)?;
            }
            other => {
                return Err(JccError::parse(format!(
                    "unknown HPROF heap subrecord tag 0x{other:02x}"
                )));
            }
        }
    }
    Ok(())
}
