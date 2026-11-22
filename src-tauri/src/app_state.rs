use crate::core::error::{JccError, Result};
use crate::core::jdk::scan_jdks;
use crate::core::models::{JdkScan, JvmProcess};
use crate::core::monitor::MonitorEngine;
use crate::core::process::{jcmd_for_process, ProcessService};
use crate::core::storage::Storage;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

/// Shared desktop state. Every long-running operation is serialised through
/// these mutexes; the UI polls, so there is never more than one attach at a
/// time and `jcmd` is not asked to serve overlapping requests for a process.
pub struct AppState {
    pub storage: Storage,
    pub data_dir: PathBuf,
    pub dumps_dir: PathBuf,
    processes: Mutex<ProcessService>,
    monitor: Mutex<MonitorEngine>,
    jdks: Mutex<Option<JdkScan>>,
}

impl AppState {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| JccError::msg(format!("could not resolve the app data directory: {err}")))?;
        let dumps_dir = data_dir.join("dumps");
        std::fs::create_dir_all(&dumps_dir)?;
        let storage = Storage::open(&data_dir.join("jcc.sqlite3"))?;
        let processes = ProcessService::new()?;
        Ok(Self {
            storage,
            data_dir,
            dumps_dir,
            processes: Mutex::new(processes),
            monitor: Mutex::new(MonitorEngine::new()),
            jdks: Mutex::new(None),
        })
    }

    pub fn lock_processes(&self) -> Result<MutexGuard<'_, ProcessService>> {
        self.processes
            .lock()
            .map_err(|_| JccError::msg("the process service lock was poisoned"))
    }

    pub fn lock_monitor(&self) -> Result<MutexGuard<'_, MonitorEngine>> {
        self.monitor
            .lock()
            .map_err(|_| JccError::msg("the monitor lock was poisoned"))
    }

    pub fn process(&self, pid: u32) -> Result<JvmProcess> {
        self.lock_processes()?
            .get(pid)
            .ok_or_else(|| JccError::NotFound(format!("Java process {pid} is not running")))
    }

    pub fn jcmd_for_pid(&self, pid: u32) -> Result<PathBuf> {
        let mut processes = self.lock_processes()?;
        let process = processes
            .get(pid)
            .ok_or_else(|| JccError::NotFound(format!("Java process {pid} is not running")))?;
        jcmd_for_process(&process).ok_or_else(|| {
            JccError::attach(format!(
                "No jcmd was found for PID {pid}. Add a full JDK to JAVA_HOME or the JDK list."
            ))
        })
    }

    /// Cached JDK scan. The first call (or an explicit rescan) walks the
    /// filesystem; later calls return the same result instantly.
    pub fn jdks(&self, force: bool) -> Result<JdkScan> {
        let mut cache = self
            .jdks
            .lock()
            .map_err(|_| JccError::msg("the JDK cache lock was poisoned"))?;
        if force || cache.is_none() {
            let settings = self.storage.settings()?;
            let scan = scan_jdks(settings.default_jdk.as_deref());
            *cache = Some(scan.clone());
        }
        Ok(cache.clone().unwrap_or_else(|| scan_jdks(None)))
    }

    pub fn invalidate_jdks(&self) -> Result<()> {
        let mut cache = self
            .jdks
            .lock()
            .map_err(|_| JccError::msg("the JDK cache lock was poisoned"))?;
        *cache = None;
        Ok(())
    }
}
