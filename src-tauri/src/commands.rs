use crate::app_state::AppState;
use crate::core::classes::read_class_loading;
use crate::core::error::{JccError, Result};
use crate::core::hprof::analyze_heap_dump;
use crate::core::jit::read_jit;
use crate::core::models::{
    AppInfo, AppSettings, ClassLoadingSnapshot, DumpInfo, GcEvent, HeapAnalysis, JdkScan,
    JfrRecording, JitSnapshot, JvmProcess, MonitorSample, RuntimeDetails, SamplePoint,
    SpringCandidate, SpringConnection, SpringSnapshot, TerminationResult, ThreadSnapshot,
};
use crate::core::process::{
    jcmd_for_process, terminate_process as terminate_java_process, ProcessService,
};
use crate::core::runtime::{
    dump_jfr as dump_jfr_file, jfr_check as check_jfr_recordings, read_runtime_details,
    start_jfr as start_jfr_recording, stop_jfr as stop_jfr_recording,
};
use crate::core::spring::{
    discover_candidates, normalize_base_url, probe_spring as probe_actuator,
};
use crate::core::threads::parse_thread_dump;
use crate::core::tools::{
    default_java_home, human_os, now_ms, open_in_file_manager, run_jcmd, stable_id,
};
use std::path::{Path, PathBuf};
use tauri::State;

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state.data_dir.display().to_string(),
        dumps_dir: state.dumps_dir.display().to_string(),
        platform: human_os(),
        arch: std::env::consts::ARCH.to_string(),
        java_home: default_java_home().map(|path| path.display().to_string()),
    }
}

#[tauri::command(async)]
pub fn scan_jdks(state: State<'_, AppState>, force: Option<bool>) -> Result<JdkScan> {
    state.jdks(force.unwrap_or(false))
}

#[tauri::command(async)]
pub fn list_processes(state: State<'_, AppState>) -> Result<Vec<JvmProcess>> {
    Ok(state.lock_processes()?.list())
}

#[tauri::command(async)]
pub fn get_process(state: State<'_, AppState>, pid: u32) -> Result<JvmProcess> {
    state.process(pid)
}

#[tauri::command(async)]
pub fn terminate_process(
    state: State<'_, AppState>,
    pid: u32,
    force: bool,
) -> Result<TerminationResult> {
    let jcmd = {
        let mut processes = state.lock_processes()?;
        let process = processes
            .get(pid)
            .ok_or_else(|| JccError::NotFound(format!("Java process {pid} is not running")))?;
        jcmd_for_process(&process)
    };
    let mut processes: std::sync::MutexGuard<'_, ProcessService> = state.lock_processes()?;
    terminate_java_process(&mut processes, jcmd.as_deref(), pid, force)
}

#[tauri::command(async)]
pub fn monitor_sample(state: State<'_, AppState>, pid: u32) -> Result<MonitorSample> {
    let os = state
        .lock_processes()?
        .os_metrics(pid)
        .ok_or_else(|| JccError::NotFound(format!("Java process {pid} is not running")))?;
    let jcmd = state.jcmd_for_pid(pid)?;
    let timestamp = now_ms();
    let outcome = state.lock_monitor()?.sample(&jcmd, pid, os, timestamp)?;
    state.storage.insert_sample(&outcome.sample)?;
    if let Some(event) = &outcome.gc_event {
        state.storage.insert_gc_event(event)?;
    }
    Ok(outcome.sample)
}

#[tauri::command(async)]
pub fn get_samples(
    state: State<'_, AppState>,
    pid: u32,
    minutes: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<SamplePoint>> {
    let minutes = minutes.unwrap_or(30).clamp(1, 1_440);
    let since = now_ms() - i64::from(minutes) * 60_000;
    let limit = limit.unwrap_or(600).clamp(1, 5_000);
    state.storage.samples(pid, since, limit)
}

#[tauri::command(async)]
pub fn get_gc_events(
    state: State<'_, AppState>,
    pid: u32,
    limit: Option<u32>,
) -> Result<Vec<GcEvent>> {
    state.storage.gc_events(pid, limit.unwrap_or(100).clamp(1, 1_000))
}

#[tauri::command(async)]
pub fn get_threads(state: State<'_, AppState>, pid: u32) -> Result<ThreadSnapshot> {
    let jcmd = state.jcmd_for_pid(pid)?;
    let text = run_jcmd(&jcmd, pid, "Thread.print", &[], 30_000)?;
    Ok(parse_thread_dump(&text, pid))
}

#[tauri::command(async)]
pub fn get_class_loading(state: State<'_, AppState>, pid: u32) -> Result<ClassLoadingSnapshot> {
    let jcmd = state.jcmd_for_pid(pid)?;
    read_class_loading(&jcmd, pid)
}

#[tauri::command(async)]
pub fn get_jit(state: State<'_, AppState>, pid: u32) -> Result<JitSnapshot> {
    let jcmd = state.jcmd_for_pid(pid)?;
    read_jit(&jcmd, pid)
}

#[tauri::command(async)]
pub fn get_runtime_details(state: State<'_, AppState>, pid: u32) -> Result<RuntimeDetails> {
    let uptime_secs = state.process(pid)?.uptime_secs;
    let jcmd = state.jcmd_for_pid(pid)?;
    read_runtime_details(&jcmd, pid, uptime_secs)
}

#[tauri::command(async)]
pub fn check_jfr(state: State<'_, AppState>, pid: u32) -> Result<Vec<JfrRecording>> {
    let jcmd = state.jcmd_for_pid(pid)?;
    check_jfr_recordings(&jcmd, pid)
}

#[tauri::command(async)]
pub fn start_jfr(
    state: State<'_, AppState>,
    pid: u32,
    name: Option<String>,
    duration_secs: Option<u64>,
) -> Result<String> {
    let jcmd = state.jcmd_for_pid(pid)?;
    let name = sanitize_name(name.unwrap_or_else(|| format!("jcc-{pid}-{}", now_ms())));
    std::fs::create_dir_all(&state.dumps_dir)?;
    let filename = state.dumps_dir.join(format!("{name}.jfr"));
    start_jfr_recording(
        &jcmd,
        pid,
        &name,
        duration_secs.unwrap_or(0),
        Some(&filename.to_string_lossy()),
    )
}

#[tauri::command(async)]
pub fn dump_jfr(state: State<'_, AppState>, pid: u32, name: String) -> Result<DumpInfo> {
    let jcmd = state.jcmd_for_pid(pid)?;
    std::fs::create_dir_all(&state.dumps_dir)?;
    let created = now_ms();
    let filename = state
        .dumps_dir
        .join(format!("{}-{created}.jfr", sanitize_name(name.clone())));
    let path_text = filename.to_string_lossy().to_string();
    let output = dump_jfr_file(&jcmd, pid, &name, &path_text)?;
    let size_bytes = std::fs::metadata(&filename).map(|meta| meta.len()).unwrap_or(0);
    let dump = DumpInfo {
        id: stable_id(&path_text),
        pid: Some(pid),
        kind: "jfr".to_string(),
        path: path_text,
        created_at: created,
        size_bytes,
        summary: Some(serde_json::json!({ "message": output.trim() })),
    };
    state.storage.save_dump(&dump)?;
    Ok(dump)
}

#[tauri::command(async)]
pub fn stop_jfr(state: State<'_, AppState>, pid: u32, name: String) -> Result<String> {
    let jcmd = state.jcmd_for_pid(pid)?;
    stop_jfr_recording(&jcmd, pid, &name)
}

#[tauri::command(async)]
pub fn capture_heap_dump(state: State<'_, AppState>, pid: u32) -> Result<DumpInfo> {
    let jcmd = state.jcmd_for_pid(pid)?;
    std::fs::create_dir_all(&state.dumps_dir)?;
    let created = now_ms();
    let mut filename = state.dumps_dir.join(format!("jcc-{pid}-{created}.hprof"));
    let mut suffix = 1;
    while filename.exists() {
        filename = state
            .dumps_dir
            .join(format!("jcc-{pid}-{created}-{suffix}.hprof"));
        suffix += 1;
    }
    let path_text = filename.to_string_lossy().to_string();
    let output = run_jcmd(
        &jcmd,
        pid,
        "GC.heap_dump",
        std::slice::from_ref(&path_text),
        180_000,
    )?;
    let size_bytes = std::fs::metadata(&filename).map(|meta| meta.len()).unwrap_or(0);
    let dump = DumpInfo {
        id: stable_id(&path_text),
        pid: Some(pid),
        kind: "heap".to_string(),
        path: path_text,
        created_at: created,
        size_bytes,
        summary: Some(serde_json::json!({ "message": output.trim() })),
    };
    state.storage.save_dump(&dump)?;
    Ok(dump)
}

#[tauri::command(async)]
pub fn analyze_heap(state: State<'_, AppState>, path: String) -> Result<HeapAnalysis> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Err(JccError::NotFound(format!("{path} does not exist")));
    }
    let analysis = analyze_heap_dump(&file)?;
    let summary = serde_json::json!({
        "totalObjects": analysis.total_objects,
        "reachableObjects": analysis.reachable_objects,
        "weakObjects": analysis.weak_objects,
        "totalShallowBytes": analysis.total_shallow_bytes,
        "classCount": analysis.class_count,
        "dominatorsComputed": analysis.dominators_computed,
    });
    // Dumps captured by JCC already have a row; external dumps simply do not
    // match and are still returned to the UI.
    let _ = state
        .storage
        .update_dump_summary_by_path(&analysis.path, &summary);
    Ok(analysis)
}

#[tauri::command(async)]
pub fn list_dumps(state: State<'_, AppState>) -> Result<Vec<DumpInfo>> {
    state.storage.list_dumps()
}

#[tauri::command(async)]
pub fn delete_dump(
    state: State<'_, AppState>,
    id: String,
    delete_file: Option<bool>,
) -> Result<()> {
    let dump = state
        .storage
        .list_dumps()?
        .into_iter()
        .find(|dump| dump.id == id);
    state.storage.delete_dump(&id)?;
    if delete_file.unwrap_or(false) {
        if let Some(dump) = dump {
            let _ = std::fs::remove_file(dump.path);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<()> {
    open_in_file_manager(Path::new(&path))
}

#[tauri::command(async)]
pub fn probe_spring(
    base_url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<SpringSnapshot> {
    probe_actuator(&base_url, username.as_deref(), password.as_deref())
}

#[tauri::command(async)]
pub fn discover_spring(state: State<'_, AppState>) -> Result<Vec<SpringCandidate>> {
    let processes = state.lock_processes()?.list();
    Ok(discover_candidates(&processes))
}

#[tauri::command(async)]
pub fn list_spring_connections(state: State<'_, AppState>) -> Result<Vec<SpringConnection>> {
    state.storage.list_spring_connections()
}

#[tauri::command(async)]
pub fn save_spring_connection(
    state: State<'_, AppState>,
    name: String,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<SpringConnection> {
    let base_url = normalize_base_url(&base_url)?;
    let name = if name.trim().is_empty() {
        base_url.clone()
    } else {
        name.trim().to_string()
    };
    state.storage.save_spring_connection(
        &name,
        &base_url,
        username.as_deref(),
        password.as_deref(),
    )
}

#[tauri::command(async)]
pub fn delete_spring_connection(state: State<'_, AppState>, id: i64) -> Result<()> {
    state.storage.delete_spring_connection(id)
}

#[tauri::command(async)]
pub fn check_spring_connection(state: State<'_, AppState>, id: i64) -> Result<SpringSnapshot> {
    let connection = state
        .storage
        .list_spring_connections()?
        .into_iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| JccError::NotFound(format!("Spring connection {id} does not exist")))?;
    let password = state.storage.spring_password(id)?;
    let snapshot = probe_actuator(
        &connection.base_url,
        connection.username.as_deref(),
        password.as_deref(),
    )?;
    state.storage.mark_spring_checked(id, now_ms())?;
    Ok(snapshot)
}

#[tauri::command(async)]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings> {
    state.storage.settings()
}

#[tauri::command(async)]
pub fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings> {
    state.storage.save_settings(&settings)?;
    state
        .storage
        .prune(settings.retention_days, settings.sample_retention_minutes)?;
    state.invalidate_jdks()?;
    Ok(settings)
}

#[tauri::command(async)]
pub fn prune_storage(state: State<'_, AppState>) -> Result<()> {
    let settings = state.storage.settings()?;
    state
        .storage
        .prune(settings.retention_days, settings.sample_retention_minutes)
}

fn sanitize_name(input: String) -> String {
    let cleaned = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "jcc-recording".to_string()
    } else {
        trimmed
    }
}
