use crate::core::error::{JccError, Result};
use crate::core::models::{
    CodeHeapInfo, GcCollectorStat, GcEvent, MemoryPool, MonitorSample, PerfRuntime,
};
use crate::core::process::OsMetrics;
use crate::core::tools::{now_ms, run_jcmd};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeCacheSnapshot {
    pub heaps: Vec<CodeHeapInfo>,
    pub total_size: u64,
    pub used: u64,
    pub max_used: u64,
    pub free: u64,
    pub nmethods: u64,
    pub blobs: u64,
    pub adapters: u64,
    pub compilation_enabled: bool,
    pub fetched_at: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub struct GcSnapshot {
    pub total_count: u64,
    pub total_time_ms: u64,
    pub heap_used: u64,
    pub collectors: Vec<(String, u64, u64)>,
}

#[derive(Debug, Clone)]
pub struct MonitorOutcome {
    pub sample: MonitorSample,
    pub gc_event: Option<GcEvent>,
}

/// Per-process monitor state. Kept in memory so JCC never needs a Java agent
/// in the target JVM: a single `jcmd PerfCounter.print` sample is enough for
/// heap, GC, threads, classes and JIT counters.
pub struct MonitorEngine {
    code_cache: HashMap<u32, CodeCacheSnapshot>,
    previous_gc: HashMap<u32, GcSnapshot>,
}

impl MonitorEngine {
    pub fn new() -> Self {
        Self {
            code_cache: HashMap::new(),
            previous_gc: HashMap::new(),
        }
    }

    pub fn code_cache(&self, pid: u32) -> Option<&CodeCacheSnapshot> {
        self.code_cache.get(&pid)
    }

    pub fn sample(
        &mut self,
        jcmd: &Path,
        pid: u32,
        os: OsMetrics,
        timestamp: i64,
    ) -> Result<MonitorOutcome> {
        let perf = match run_jcmd(jcmd, pid, "PerfCounter.print", &[], 10_000) {
            Ok(text) => text,
            Err(err) => {
                return Ok(MonitorOutcome {
                    sample: os_only_sample(pid, os, timestamp, err.to_string()),
                    gc_event: None,
                });
            }
        };
        let counters = parse_perf_counters(&perf);

        let needs_code_cache = self
            .code_cache
            .get(&pid)
            .and_then(|snapshot| snapshot.fetched_at)
            .map(|fetched| fetched.elapsed() > Duration::from_secs(5))
            .unwrap_or(true);
        if needs_code_cache {
            if let Ok(text) = run_jcmd(jcmd, pid, "Compiler.codecache", &[], 8_000) {
                if let Ok(mut snapshot) = parse_code_cache(&text) {
                    snapshot.fetched_at = Some(Instant::now());
                    self.code_cache.insert(pid, snapshot);
                }
            }
        }

        let code_cache = self.code_cache.get(&pid).cloned();
        let sample = build_sample(
            pid,
            timestamp,
            &counters,
            &os,
            code_cache.as_ref(),
            None,
        );

        let current_gc = GcSnapshot {
            total_count: sample.gc_count,
            total_time_ms: sample.gc_time_ms,
            heap_used: sample.heap_used,
            collectors: sample
                .gc_collectors
                .iter()
                .map(|collector| {
                    (
                        collector.name.clone(),
                        collector.invocations,
                        collector.time_ms,
                    )
                })
                .collect(),
        };

        let gc_event = self
            .previous_gc
            .get(&pid)
            .and_then(|previous| gc_event_between(pid, previous, &current_gc, timestamp, sample.gc_last_cause.clone()));
        self.previous_gc.insert(pid, current_gc);

        Ok(MonitorOutcome { sample, gc_event })
    }

    pub fn reset(&mut self, pid: u32) {
        self.code_cache.remove(&pid);
        self.previous_gc.remove(&pid);
    }
}

impl Default for MonitorEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn os_only_sample(pid: u32, os: OsMetrics, timestamp: i64, error: String) -> MonitorSample {
    MonitorSample {
        pid,
        timestamp,
        cpu_percent: os.cpu_percent,
        rss_bytes: os.rss_bytes,
        virtual_bytes: os.virtual_bytes,
        uptime_secs: os.uptime_secs,
        source: "os".to_string(),
        attach_error: Some(error),
        ..Default::default()
    }
}

pub fn parse_perf_counters(text: &str) -> BTreeMap<String, String> {
    let mut counters = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.ends_with(':') && !line.contains('=') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        let mut value = value.trim();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value = &value[1..value.len() - 1];
        }
        counters.insert(key.to_string(), value.to_string());
    }
    counters
}

fn number(counters: &BTreeMap<String, String>, key: &str) -> u64 {
    counters
        .get(key)
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| value.max(0.0) as u64)
        .unwrap_or(0)
}

fn text(counters: &BTreeMap<String, String>, key: &str) -> Option<String> {
    counters
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn ticks_to_ms(counters: &BTreeMap<String, String>, ticks: u64) -> u64 {
    let frequency = counters
        .get("sun.os.hrt.frequency")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0)
        .unwrap_or(10_000_000.0);
    ((ticks as f64) * 1000.0 / frequency).round() as u64
}

pub fn build_sample(
    pid: u32,
    timestamp: i64,
    counters: &BTreeMap<String, String>,
    os: &OsMetrics,
    code_cache: Option<&CodeCacheSnapshot>,
    attach_error: Option<String>,
) -> MonitorSample {
    let mut gc_collectors = Vec::new();
    let mut index = 0u32;
    loop {
        let prefix = format!("sun.gc.collector.{index}");
        let Some(name) = text(counters, &format!("{prefix}.name")) else {
            break;
        };
        let invocations = number(counters, &format!("{prefix}.invocations"));
        let time_ticks = number(counters, &format!("{prefix}.time"));
        let last_entry = number(counters, &format!("{prefix}.lastEntryTime"));
        let last_exit = number(counters, &format!("{prefix}.lastExitTime"));
        gc_collectors.push(GcCollectorStat {
            index,
            name,
            invocations,
            time_ms: ticks_to_ms(counters, time_ticks),
            last_pause_ms: ticks_to_ms(counters, last_exit.saturating_sub(last_entry)),
        });
        index += 1;
    }

    let gc_count = gc_collectors.iter().map(|collector| collector.invocations).sum();
    let gc_time_ms = gc_collectors.iter().map(|collector| collector.time_ms).sum();

    let mut heap_used = 0u64;
    let mut heap_committed = 0u64;
    let mut heap_max = 0u64;
    let mut generation = 0u32;
    while counters.contains_key(&format!("sun.gc.generation.{generation}.capacity")) {
        heap_committed += number(counters, &format!("sun.gc.generation.{generation}.capacity"));
        heap_max += number(
            counters,
            &format!("sun.gc.generation.{generation}.maxCapacity"),
        );
        generation += 1;
    }
    for key in counters.keys() {
        if key.starts_with("sun.gc.generation.") && key.ends_with(".used") {
            heap_used += number(counters, key);
        }
    }

    let eden = find_space(counters, "eden");
    let survivor = find_survivor_space(counters);
    let old = find_space(counters, "space").or_else(|| find_space(counters, "old"));

    let metaspace_used = number(counters, "sun.gc.metaspace.used");
    let metaspace_committed = number(counters, "sun.gc.metaspace.capacity");
    let metaspace_max = number(counters, "sun.gc.metaspace.maxCapacity");
    let class_space_used = number(counters, "sun.gc.compressedclassspace.used");
    let class_space_committed = number(counters, "sun.gc.compressedclassspace.capacity");
    let class_space_max = number(counters, "sun.gc.compressedclassspace.maxCapacity");
    let code_used = code_cache.map(|snapshot| snapshot.used).unwrap_or(0);
    let code_committed = code_cache.map(|snapshot| snapshot.total_size).unwrap_or(0);
    let code_max = code_cache.map(|snapshot| snapshot.total_size).unwrap_or(0);

    let classes_loaded =
        number(counters, "java.cls.loadedClasses") + number(counters, "java.cls.sharedLoadedClasses");
    let classes_unloaded = number(counters, "java.cls.unloadedClasses")
        + number(counters, "java.cls.sharedUnloadedClasses");
    let classes_current = classes_loaded.saturating_sub(classes_unloaded);

    let mut memory_pools = Vec::new();
    if let Some((used, committed, max)) = eden {
        memory_pools.push(MemoryPool {
            name: "Eden Space".to_string(),
            kind: "heap".to_string(),
            used,
            committed,
            max,
        });
    }
    if let Some((used, committed, max)) = survivor {
        memory_pools.push(MemoryPool {
            name: "Survivor Space".to_string(),
            kind: "heap".to_string(),
            used,
            committed,
            max,
        });
    }
    if let Some((used, committed, max)) = old {
        memory_pools.push(MemoryPool {
            name: "Old Gen".to_string(),
            kind: "heap".to_string(),
            used,
            committed,
            max,
        });
    }
    if metaspace_max > 0 || metaspace_used > 0 {
        memory_pools.push(MemoryPool {
            name: "Metaspace".to_string(),
            kind: "nonheap".to_string(),
            used: metaspace_used,
            committed: metaspace_committed,
            max: metaspace_max,
        });
    }
    if class_space_max > 0 || class_space_used > 0 {
        memory_pools.push(MemoryPool {
            name: "Compressed Class Space".to_string(),
            kind: "nonheap".to_string(),
            used: class_space_used,
            committed: class_space_committed,
            max: class_space_max,
        });
    }
    if code_max > 0 {
        memory_pools.push(MemoryPool {
            name: "Code Cache".to_string(),
            kind: "code".to_string(),
            used: code_used,
            committed: code_committed,
            max: code_max,
        });
    }

    let runtime = Some(PerfRuntime {
        java_version: text(counters, "java.property.java.version"),
        java_home: text(counters, "java.property.java.home"),
        vm_name: text(counters, "java.property.java.vm.name"),
        vm_vendor: text(counters, "java.property.java.vm.vendor"),
        vm_version: text(counters, "java.property.java.vm.version"),
        vm_info: text(counters, "java.property.java.vm.info"),
        java_command: text(counters, "sun.rt.javaCommand"),
        vm_args: text(counters, "java.rt.vmArgs"),
        class_path: text(counters, "java.property.java.class.path"),
    });

    let source = if heap_max == 0 && counters.contains_key("sun.gc.collector.0.name") {
        "perf-partial"
    } else if counters.is_empty() {
        "os"
    } else {
        "perf"
    };

    MonitorSample {
        pid,
        timestamp,
        cpu_percent: os.cpu_percent,
        rss_bytes: os.rss_bytes,
        virtual_bytes: os.virtual_bytes,
        uptime_secs: os.uptime_secs,
        heap_used,
        heap_committed,
        heap_max,
        non_heap_used: metaspace_used + class_space_used + code_used,
        non_heap_committed: metaspace_committed + class_space_committed + code_committed,
        eden_used: eden.map(|(used, _, _)| used).unwrap_or(0),
        eden_committed: eden.map(|(_, committed, _)| committed).unwrap_or(0),
        survivor_used: survivor.map(|(used, _, _)| used).unwrap_or(0),
        survivor_committed: survivor.map(|(_, committed, _)| committed).unwrap_or(0),
        old_used: old.map(|(used, _, _)| used).unwrap_or(0),
        old_committed: old.map(|(_, committed, _)| committed).unwrap_or(0),
        metaspace_used,
        metaspace_committed,
        metaspace_max,
        compressed_class_space_used: class_space_used,
        compressed_class_space_committed: class_space_committed,
        compressed_class_space_max: class_space_max,
        code_cache_used: code_used,
        code_cache_committed: code_committed,
        code_cache_max: code_max,
        threads_live: number(counters, "java.threads.live") as u32,
        threads_daemon: number(counters, "java.threads.daemon") as u32,
        threads_peak: number(counters, "java.threads.livePeak") as u32,
        threads_started: number(counters, "java.threads.started"),
        classes_loaded,
        classes_unloaded,
        classes_current,
        gc_count,
        gc_time_ms,
        gc_last_cause: text(counters, "sun.gc.lastCause")
            .or_else(|| text(counters, "sun.gc.cause")),
        gc_collectors,
        jit_compiles: number(counters, "sun.ci.totalCompiles"),
        jit_time_ms: ticks_to_ms(counters, number(counters, "java.ci.totalTime")),
        jit_bailouts: number(counters, "sun.ci.totalBailouts"),
        jit_invalidates: number(counters, "sun.ci.totalInvalidates"),
        // PerfCounter has no installed-nmethod count: the CodeCache output is
        // the authoritative source for `nmethods=`. nmethodCodeSize is the
        // native machine-code size in bytes.
        jit_nmethods: code_cache.map(|snapshot| snapshot.nmethods).unwrap_or(0),
        jit_nmethod_size: number(counters, "sun.ci.nmethodCodeSize"),
        memory_pools,
        runtime,
        attach_error,
        source: source.to_string(),
    }
}

fn find_space(
    counters: &BTreeMap<String, String>,
    wanted: &str,
) -> Option<(u64, u64, u64)> {
    for generation in 0..4 {
        let spaces = number(counters, &format!("sun.gc.generation.{generation}.spaces"));
        for space in 0..spaces.max(1) {
            let prefix = format!("sun.gc.generation.{generation}.space.{space}");
            let Some(name) = text(counters, &format!("{prefix}.name")) else {
                continue;
            };
            if name == wanted {
                return Some((
                    number(counters, &format!("{prefix}.used")),
                    number(counters, &format!("{prefix}.capacity")),
                    number(counters, &format!("{prefix}.maxCapacity")),
                ));
            }
        }
    }
    None
}

fn find_survivor_space(counters: &BTreeMap<String, String>) -> Option<(u64, u64, u64)> {
    let mut used = 0;
    let mut committed = 0;
    let mut max = 0;
    let mut found = false;
    for generation in 0..4 {
        let spaces = number(counters, &format!("sun.gc.generation.{generation}.spaces"));
        for space in 0..spaces.max(1) {
            let prefix = format!("sun.gc.generation.{generation}.space.{space}");
            let Some(name) = text(counters, &format!("{prefix}.name")) else {
                continue;
            };
            if name == "s0" || name == "s1" || name.to_ascii_lowercase().contains("survivor") {
                used += number(counters, &format!("{prefix}.used"));
                committed += number(counters, &format!("{prefix}.capacity"));
                max += number(counters, &format!("{prefix}.maxCapacity"));
                found = true;
            }
        }
    }
    found.then_some((used, committed, max))
}

pub fn parse_code_cache(text: &str) -> Result<CodeCacheSnapshot> {
    let mut snapshot = CodeCacheSnapshot::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("CodeHeap '") {
            let Some(name_end) = trimmed.find('\'') else {
                continue;
            };
            let name = trimmed[..name_end].trim_start_matches("CodeHeap '").to_string();
            let mut heap = CodeHeapInfo {
                name,
                size: 0,
                used: 0,
                max_used: 0,
                free: 0,
            };
            for part in trimmed.split_whitespace() {
                let Some((key, value)) = part.split_once('=') else {
                    continue;
                };
                let bytes = parse_byte_size(value).unwrap_or(0);
                match key {
                    "size" => heap.size = bytes,
                    "used" => heap.used = bytes,
                    "max_used" => heap.max_used = bytes,
                    "free" => heap.free = bytes,
                    _ => {}
                }
            }
            snapshot.total_size += heap.size;
            snapshot.used += heap.used;
            snapshot.max_used += heap.max_used;
            snapshot.free += heap.free;
            snapshot.heaps.push(heap);
        } else if trimmed.starts_with("total_blobs=") {
            for part in trimmed.split_whitespace() {
                let Some((key, value)) = part.split_once('=') else {
                    continue;
                };
                match key {
                    "total_blobs" => snapshot.blobs = value.parse().unwrap_or(0),
                    "nmethods" => snapshot.nmethods = value.parse().unwrap_or(0),
                    "adapters" => snapshot.adapters = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        } else if trimmed.starts_with("compilation:") {
            snapshot.compilation_enabled = trimmed.to_ascii_lowercase().contains("enabled");
        }
    }
    if snapshot.heaps.is_empty() {
        return Err(JccError::parse(
            "Compiler.codecache did not contain any code heap information",
        ));
    }
    Ok(snapshot)
}

fn parse_byte_size(value: &str) -> Option<u64> {
    let value = value.trim();
    let split = value
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(value.len());
    let number: f64 = value[..split].parse().ok()?;
    let unit = value[split..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    Some((number * multiplier).round() as u64)
}

fn gc_event_between(
    pid: u32,
    previous: &GcSnapshot,
    current: &GcSnapshot,
    timestamp: i64,
    cause: Option<String>,
) -> Option<GcEvent> {
    if current.total_count <= previous.total_count {
        return None;
    }
    let invocations = current.total_count - previous.total_count;
    let duration_ms = current.total_time_ms.saturating_sub(previous.total_time_ms);
    let mut changed = Vec::new();
    for (name, count, _) in &current.collectors {
        if let Some((_, previous_count, _)) = previous
            .collectors
            .iter()
            .find(|(previous_name, _, _)| previous_name == name)
        {
            if count > previous_count {
                changed.push(name.clone());
            }
        }
    }
    Some(GcEvent {
        id: 0,
        pid,
        timestamp,
        collector: if changed.is_empty() {
            "Garbage collection".to_string()
        } else {
            changed.join(", ")
        },
        invocations,
        duration_ms,
        heap_before: previous.heap_used,
        heap_after: current.heap_used,
        reclaimed_bytes: previous.heap_used.saturating_sub(current.heap_used),
        cause,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERF: &str = r#"
12345:
java.cls.loadedClasses=10
java.cls.sharedLoadedClasses=585
java.cls.unloadedClasses=2
java.cls.sharedUnloadedClasses=0
java.property.java.version="21.0.9"
java.property.java.home="C:\Program Files\Java\jdk-21"
java.property.java.vm.name="Java HotSpot(TM) 64-Bit Server VM"
java.property.java.vm.vendor="Oracle Corporation"
java.property.java.vm.version="21.0.9+7-LTS-338"
java.property.java.vm.info="mixed mode, sharing"
java.property.java.class.path="C:\work\classes"
java.rt.vmArgs="-Xmx512m -XX:+UseG1GC"
java.threads.daemon=6
java.threads.live=9
java.threads.livePeak=12
java.threads.started=14
java.ci.totalTime=475183
sun.ci.totalCompiles=73
sun.ci.totalBailouts=1
sun.ci.totalInvalidates=2
sun.ci.nmethodCodeSize=51992
sun.ci.nmethodSize=72416
sun.gc.cause="No GC"
sun.gc.lastCause="G1 Evacuation Pause"
sun.gc.collector.0.invocations=5
sun.gc.collector.0.lastEntryTime=2885475
sun.gc.collector.0.lastExitTime=2978393
sun.gc.collector.0.name="G1 young collection pauses"
sun.gc.collector.0.time=508345
sun.gc.collector.1.invocations=0
sun.gc.collector.1.lastEntryTime=0
sun.gc.collector.1.lastExitTime=0
sun.gc.collector.1.name="G1 full collection pauses"
sun.gc.collector.1.time=0
sun.gc.compressedclassspace.capacity=131072
sun.gc.compressedclassspace.maxCapacity=1073741824
sun.gc.compressedclassspace.used=7928
sun.gc.generation.0.capacity=87031808
sun.gc.generation.0.maxCapacity=536870912
sun.gc.generation.0.spaces=3
sun.gc.generation.0.space.0.capacity=83886080
sun.gc.generation.0.space.0.maxCapacity=536870912
sun.gc.generation.0.space.0.name="eden"
sun.gc.generation.0.space.0.used=42991616
sun.gc.generation.0.space.1.capacity=0
sun.gc.generation.0.space.1.name="s0"
sun.gc.generation.0.space.1.used=0
sun.gc.generation.0.space.2.capacity=3145728
sun.gc.generation.0.space.2.maxCapacity=536870912
sun.gc.generation.0.space.2.name="s1"
sun.gc.generation.0.space.2.used=3145728
sun.gc.generation.1.capacity=206569472
sun.gc.generation.1.maxCapacity=536870912
sun.gc.generation.1.spaces=1
sun.gc.generation.1.space.0.capacity=206569472
sun.gc.generation.1.space.0.maxCapacity=536870912
sun.gc.generation.1.space.0.name="space"
sun.gc.generation.1.space.0.used=92799008
sun.gc.metaspace.capacity=393216
sun.gc.metaspace.maxCapacity=1140850688
sun.gc.metaspace.used=194776
sun.os.hrt.frequency=10000000
sun.rt.javaCommand="Probe"
"#;

    const CODE_CACHE: &str = r#"
CodeHeap 'non-profiled nmethods': size=120000Kb used=30Kb max_used=30Kb free=119969Kb
 bounds [0x1, 0x2, 0x3]
CodeHeap 'profiled nmethods': size=120000Kb used=98Kb max_used=98Kb free=119901Kb
 bounds [0x1, 0x2, 0x3]
CodeHeap 'non-nmethods': size=5760Kb used=1239Kb max_used=1239Kb free=4520Kb
 bounds [0x1, 0x2, 0x3]
 total_blobs=456 nmethods=93 adapters=268
 compilation: enabled
"#;

    #[test]
    fn parses_perf_counters_and_builds_sample() {
        let counters = parse_perf_counters(PERF);
        assert_eq!(counters.get("java.property.java.version").map(String::as_str), Some("21.0.9"));
        assert_eq!(counters.get("sun.gc.generation.0.space.0.name").map(String::as_str), Some("eden"));
        let cache = parse_code_cache(CODE_CACHE).unwrap();
        let os = OsMetrics {
            cpu_percent: 12.5,
            rss_bytes: 1234,
            virtual_bytes: 5678,
            uptime_secs: 100,
            start_time: 0,
        };
        let sample = build_sample(1, 42, &counters, &os, Some(&cache), None);
        assert_eq!(sample.heap_used, 42_991_616 + 3_145_728 + 92_799_008);
        assert_eq!(sample.heap_committed, 87_031_808 + 206_569_472);
        assert_eq!(sample.heap_max, 1_073_741_824);
        assert_eq!(sample.gc_count, 5);
        assert_eq!(sample.gc_time_ms, 51);
        assert_eq!(sample.threads_live, 9);
        assert_eq!(sample.classes_current, 593);
        assert_eq!(sample.eden_used, 42_991_616);
        assert_eq!(sample.survivor_used, 3_145_728);
        assert_eq!(sample.old_used, 92_799_008);
        assert_eq!(sample.code_cache_used, 1367 * 1024);
        assert_eq!(sample.jit_compiles, 73);
        assert_eq!(sample.jit_time_ms, 48);
        assert_eq!(sample.source, "perf");
        assert_eq!(sample.gc_last_cause.as_deref(), Some("G1 Evacuation Pause"));
        assert!(sample.memory_pools.iter().any(|pool| pool.name == "Code Cache"));
    }

    #[test]
    fn parses_code_cache_totals() {
        let cache = parse_code_cache(CODE_CACHE).unwrap();
        assert_eq!(cache.heaps.len(), 3);
        assert_eq!(cache.total_size, 245_760 * 1024);
        assert_eq!(cache.nmethods, 93);
        assert!(cache.compilation_enabled);
    }

    #[test]
    fn byte_sizes_support_units() {
        assert_eq!(parse_byte_size("120000Kb"), Some(122_880_000));
        assert_eq!(parse_byte_size("2M"), Some(2 * 1024 * 1024));
        assert_eq!(parse_byte_size("512"), Some(512));
    }

    #[test]
    fn creates_gc_event_from_counter_delta() {
        let previous = GcSnapshot {
            total_count: 5,
            total_time_ms: 50,
            heap_used: 140_000_000,
            collectors: vec![("G1 young collection pauses".to_string(), 5, 50)],
        };
        let current = GcSnapshot {
            total_count: 7,
            total_time_ms: 80,
            heap_used: 90_000_000,
            collectors: vec![("G1 young collection pauses".to_string(), 7, 80)],
        };
        let event = gc_event_between(1, &previous, &current, 100, Some("G1 Evacuation Pause".to_string()))
            .expect("event");
        assert_eq!(event.invocations, 2);
        assert_eq!(event.duration_ms, 30);
        assert_eq!(event.reclaimed_bytes, 50_000_000);
    }
}
