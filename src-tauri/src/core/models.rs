use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Timestamp = i64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JdkTool {
    pub name: String,
    pub path: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JdkInfo {
    pub id: String,
    pub java_home: String,
    pub java_executable: String,
    pub javac_executable: Option<String>,
    pub version: Option<String>,
    pub version_major: Option<u32>,
    pub vendor: Option<String>,
    pub vm_name: Option<String>,
    pub architecture: Option<String>,
    pub os_arch: Option<String>,
    pub source: String,
    pub is_default: bool,
    pub tools: Vec<JdkTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JdkScan {
    pub jdks: Vec<JdkInfo>,
    pub default_java_home: Option<String>,
    pub scanned_at: Timestamp,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmProcess {
    pub pid: u32,
    pub name: String,
    pub display_name: String,
    pub main_class: Option<String>,
    pub main_jar: Option<String>,
    pub java_home: Option<String>,
    pub java_version: Option<String>,
    pub vendor: Option<String>,
    pub command_line: String,
    pub args: Vec<String>,
    pub jvm_args: Vec<String>,
    pub app_args: Vec<String>,
    pub uptime_secs: u64,
    pub start_time: u64,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
    pub executable: Option<String>,
    pub cwd: Option<String>,
    pub spring_ports: Vec<u16>,
    pub attachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDetails {
    pub pid: u32,
    pub properties: BTreeMap<String, String>,
    pub jvm_args: Vec<String>,
    pub java_command: Option<String>,
    pub class_path: Option<String>,
    pub launcher_type: Option<String>,
    pub flags: Vec<String>,
    pub version_lines: Vec<String>,
    pub uptime_secs: u64,
    pub jfr_recordings: Vec<JfrRecording>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GcCollectorStat {
    pub index: u32,
    pub name: String,
    pub invocations: u64,
    pub time_ms: u64,
    pub last_pause_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPool {
    pub name: String,
    pub kind: String,
    pub used: u64,
    pub committed: u64,
    pub max: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerfRuntime {
    pub java_version: Option<String>,
    pub java_home: Option<String>,
    pub vm_name: Option<String>,
    pub vm_vendor: Option<String>,
    pub vm_version: Option<String>,
    pub vm_info: Option<String>,
    pub java_command: Option<String>,
    pub vm_args: Option<String>,
    pub class_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSample {
    pub pid: u32,
    pub timestamp: Timestamp,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
    pub uptime_secs: u64,
    pub heap_used: u64,
    pub heap_committed: u64,
    pub heap_max: u64,
    pub non_heap_used: u64,
    pub non_heap_committed: u64,
    pub eden_used: u64,
    pub eden_committed: u64,
    pub survivor_used: u64,
    pub survivor_committed: u64,
    pub old_used: u64,
    pub old_committed: u64,
    pub metaspace_used: u64,
    pub metaspace_committed: u64,
    pub metaspace_max: u64,
    pub compressed_class_space_used: u64,
    pub compressed_class_space_committed: u64,
    pub compressed_class_space_max: u64,
    pub code_cache_used: u64,
    pub code_cache_committed: u64,
    pub code_cache_max: u64,
    pub threads_live: u32,
    pub threads_daemon: u32,
    pub threads_peak: u32,
    pub threads_started: u64,
    pub classes_loaded: u64,
    pub classes_unloaded: u64,
    pub classes_current: u64,
    pub gc_count: u64,
    pub gc_time_ms: u64,
    pub gc_last_cause: Option<String>,
    pub gc_collectors: Vec<GcCollectorStat>,
    pub jit_compiles: u64,
    pub jit_time_ms: u64,
    pub jit_bailouts: u64,
    pub jit_invalidates: u64,
    pub jit_nmethods: u64,
    pub jit_nmethod_size: u64,
    pub memory_pools: Vec<MemoryPool>,
    pub runtime: Option<PerfRuntime>,
    pub attach_error: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SamplePoint {
    pub timestamp: Timestamp,
    pub cpu_percent: f32,
    pub heap_used: u64,
    pub heap_committed: u64,
    pub heap_max: u64,
    pub non_heap_used: u64,
    pub metaspace_used: u64,
    pub code_cache_used: u64,
    pub threads_live: u32,
    pub classes_current: u64,
    pub gc_count: u64,
    pub gc_time_ms: u64,
    pub jit_compiles: u64,
    pub jit_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcEvent {
    pub id: i64,
    pub pid: u32,
    pub timestamp: Timestamp,
    pub collector: String,
    pub invocations: u64,
    pub duration_ms: u64,
    pub heap_before: u64,
    pub heap_after: u64,
    pub reclaimed_bytes: u64,
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub name: String,
    pub number: Option<u64>,
    pub daemon: bool,
    pub priority: Option<i32>,
    pub os_priority: Option<i32>,
    pub cpu_ms: f64,
    pub elapsed_secs: f64,
    pub tid: Option<String>,
    pub nid: Option<String>,
    pub state: String,
    pub state_detail: Option<String>,
    pub header: String,
    pub stack: Vec<String>,
    pub locks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSnapshot {
    pub pid: u32,
    pub timestamp: Timestamp,
    pub raw: String,
    pub threads: Vec<ThreadInfo>,
    pub counts: BTreeMap<String, u32>,
    pub deadlock: bool,
    pub deadlock_text: Option<String>,
    pub total: u32,
    pub generated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClassLoaderInfo {
    pub id: String,
    pub parent_id: String,
    pub cld: String,
    pub classes: u64,
    pub chunk_size: u64,
    pub block_size: u64,
    pub kind: String,
    pub hidden_classes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClassHistogramRow {
    pub rank: u32,
    pub instances: u64,
    pub bytes: u64,
    pub class_name: String,
    pub module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassLoadingSnapshot {
    pub pid: u32,
    pub timestamp: Timestamp,
    pub loaders: Vec<ClassLoaderInfo>,
    pub total_classes: u64,
    pub total_chunks: u64,
    pub total_blocks: u64,
    pub histogram: Vec<ClassHistogramRow>,
    pub total_instances: u64,
    pub total_bytes: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodeHeapInfo {
    pub name: String,
    pub size: u64,
    pub used: u64,
    pub max_used: u64,
    pub free: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompiledMethod {
    pub id: u64,
    pub level: u32,
    pub sync: u32,
    pub method: String,
    pub address_range: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JitSnapshot {
    pub pid: u32,
    pub timestamp: Timestamp,
    pub compiles: u64,
    pub time_ms: u64,
    pub bailouts: u64,
    pub invalidates: u64,
    pub nmethods: u64,
    pub nmethod_code_size: u64,
    pub code_heaps: Vec<CodeHeapInfo>,
    pub code_total: u64,
    pub code_used: u64,
    pub code_max_used: u64,
    pub code_free: u64,
    pub compilation_enabled: bool,
    pub compiled_methods: Vec<CompiledMethod>,
    pub total_compiled_methods: u64,
    pub compiler_threads: u32,
    pub queue: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JfrRecording {
    pub id: u32,
    pub name: Option<String>,
    pub state: Option<String>,
    pub duration: Option<String>,
    pub destination: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpInfo {
    pub id: String,
    pub pid: Option<u32>,
    pub kind: String,
    pub path: String,
    pub created_at: Timestamp,
    pub size_bytes: u64,
    pub summary: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationResult {
    pub pid: u32,
    pub force: bool,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringCandidate {
    pub pid: u32,
    pub port: u16,
    pub url: String,
    pub detected_from: String,
    pub name: String,
    pub reachable: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpringMeasurement {
    pub statistic: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringMetric {
    pub name: String,
    pub description: Option<String>,
    pub base_unit: Option<String>,
    pub measurements: Vec<SpringMeasurement>,
    pub available_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringSnapshot {
    pub base_url: String,
    pub checked_at: Timestamp,
    pub health: serde_json::Value,
    pub info: serde_json::Value,
    pub links: serde_json::Value,
    pub metrics: Vec<SpringMetric>,
    pub latency_ms: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpringConnection {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub username: Option<String>,
    pub created_at: Timestamp,
    pub last_checked_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeapClassRow {
    pub class_name: String,
    pub instances: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeapObjectRow {
    pub object_id: String,
    pub class_name: String,
    pub shallow_bytes: u64,
    pub retained_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HeapLoaderRow {
    pub loader: String,
    pub objects: u64,
    pub bytes: u64,
    pub classes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RootCountRow {
    pub kind: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapAnalysis {
    pub path: String,
    pub file_size: u64,
    pub dump_timestamp: Option<Timestamp>,
    pub id_size: u8,
    pub total_objects: u64,
    pub total_shallow_bytes: u64,
    pub class_count: u64,
    pub reachable_objects: u64,
    pub reachable_bytes: u64,
    pub weak_objects: u64,
    pub weak_bytes: u64,
    pub classes: Vec<HeapClassRow>,
    pub weak_classes: Vec<HeapClassRow>,
    pub loaders: Vec<HeapLoaderRow>,
    pub top_objects: Vec<HeapObjectRow>,
    pub dominators: Vec<HeapObjectRow>,
    pub root_counts: Vec<RootCountRow>,
    pub analysis_ms: u64,
    pub dominators_computed: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub dumps_dir: String,
    pub platform: String,
    pub arch: String,
    pub java_home: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_jdk: Option<String>,
    pub refresh_interval_ms: u64,
    pub retention_days: u32,
    pub sample_retention_minutes: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_jdk: None,
            refresh_interval_ms: 2_000,
            retention_days: 7,
            sample_retention_minutes: 30,
        }
    }
}
