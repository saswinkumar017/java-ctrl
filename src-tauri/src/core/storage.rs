use crate::core::error::{JccError, Result};
use crate::core::models::{
    AppSettings, DumpInfo, GcEvent, MonitorSample, SamplePoint, SpringConnection,
};
use crate::core::tools::now_ms;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Local history store. Samples are intentionally small and flat so the
/// monitor can write one row per poll without ever blocking on a migration.
pub struct Storage {
    connection: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;

             CREATE TABLE IF NOT EXISTS samples (
                 pid INTEGER NOT NULL,
                 ts INTEGER NOT NULL,
                 cpu_percent REAL NOT NULL,
                 heap_used INTEGER NOT NULL,
                 heap_committed INTEGER NOT NULL,
                 heap_max INTEGER NOT NULL,
                 non_heap_used INTEGER NOT NULL,
                 metaspace_used INTEGER NOT NULL,
                 code_cache_used INTEGER NOT NULL,
                 threads_live INTEGER NOT NULL,
                 classes_current INTEGER NOT NULL,
                 gc_count INTEGER NOT NULL,
                 gc_time_ms INTEGER NOT NULL,
                 jit_compiles INTEGER NOT NULL,
                 jit_time_ms INTEGER NOT NULL,
                 PRIMARY KEY (pid, ts)
             );
             CREATE INDEX IF NOT EXISTS idx_samples_ts ON samples (ts);

             CREATE TABLE IF NOT EXISTS gc_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 pid INTEGER NOT NULL,
                 ts INTEGER NOT NULL,
                 collector TEXT NOT NULL,
                 invocations INTEGER NOT NULL,
                 duration_ms INTEGER NOT NULL,
                 heap_before INTEGER NOT NULL,
                 heap_after INTEGER NOT NULL,
                 reclaimed_bytes INTEGER NOT NULL,
                 cause TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_gc_events_pid ON gc_events (pid, id);

             CREATE TABLE IF NOT EXISTS dumps (
                 id TEXT PRIMARY KEY,
                 pid INTEGER,
                 kind TEXT NOT NULL,
                 path TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 summary TEXT
             );

             CREATE TABLE IF NOT EXISTS spring_connections (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 base_url TEXT NOT NULL UNIQUE,
                 username TEXT,
                 password TEXT,
                 created_at INTEGER NOT NULL,
                 last_checked_at INTEGER
             );

             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| JccError::msg("the local database lock was poisoned"))
    }

    pub fn insert_sample(&self, sample: &MonitorSample) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR REPLACE INTO samples (
                pid, ts, cpu_percent, heap_used, heap_committed, heap_max,
                non_heap_used, metaspace_used, code_cache_used, threads_live,
                classes_current, gc_count, gc_time_ms, jit_compiles, jit_time_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                sample.pid,
                sample.timestamp,
                sample.cpu_percent as f64,
                sample.heap_used as i64,
                sample.heap_committed as i64,
                sample.heap_max as i64,
                sample.non_heap_used as i64,
                sample.metaspace_used as i64,
                sample.code_cache_used as i64,
                sample.threads_live,
                sample.classes_current as i64,
                sample.gc_count as i64,
                sample.gc_time_ms as i64,
                sample.jit_compiles as i64,
                sample.jit_time_ms as i64,
            ],
        )?;
        Ok(())
    }

    pub fn insert_gc_event(&self, event: &GcEvent) -> Result<i64> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO gc_events (
                pid, ts, collector, invocations, duration_ms, heap_before,
                heap_after, reclaimed_bytes, cause
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.pid,
                event.timestamp,
                event.collector,
                event.invocations as i64,
                event.duration_ms as i64,
                event.heap_before as i64,
                event.heap_after as i64,
                event.reclaimed_bytes as i64,
                event.cause,
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn samples(&self, pid: u32, since: i64, limit: u32) -> Result<Vec<SamplePoint>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT ts, cpu_percent, heap_used, heap_committed, heap_max,
                    non_heap_used, metaspace_used, code_cache_used, threads_live,
                    classes_current, gc_count, gc_time_ms, jit_compiles, jit_time_ms
             FROM samples
             WHERE pid = ?1 AND ts >= ?2
             ORDER BY ts DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(params![pid, since, limit], |row| {
            Ok(SamplePoint {
                timestamp: row.get(0)?,
                cpu_percent: row.get::<_, f64>(1)? as f32,
                heap_used: row.get::<_, i64>(2)? as u64,
                heap_committed: row.get::<_, i64>(3)? as u64,
                heap_max: row.get::<_, i64>(4)? as u64,
                non_heap_used: row.get::<_, i64>(5)? as u64,
                metaspace_used: row.get::<_, i64>(6)? as u64,
                code_cache_used: row.get::<_, i64>(7)? as u64,
                threads_live: row.get::<_, i64>(8)? as u32,
                classes_current: row.get::<_, i64>(9)? as u64,
                gc_count: row.get::<_, i64>(10)? as u64,
                gc_time_ms: row.get::<_, i64>(11)? as u64,
                jit_compiles: row.get::<_, i64>(12)? as u64,
                jit_time_ms: row.get::<_, i64>(13)? as u64,
            })
        })?;
        let mut points = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        points.reverse();
        Ok(points)
    }

    pub fn gc_events(&self, pid: u32, limit: u32) -> Result<Vec<GcEvent>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, pid, ts, collector, invocations, duration_ms, heap_before,
                    heap_after, reclaimed_bytes, cause
             FROM gc_events
             WHERE pid = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![pid, limit], |row| {
            Ok(GcEvent {
                id: row.get(0)?,
                pid: row.get(1)?,
                timestamp: row.get(2)?,
                collector: row.get(3)?,
                invocations: row.get::<_, i64>(4)? as u64,
                duration_ms: row.get::<_, i64>(5)? as u64,
                heap_before: row.get::<_, i64>(6)? as u64,
                heap_after: row.get::<_, i64>(7)? as u64,
                reclaimed_bytes: row.get::<_, i64>(8)? as u64,
                cause: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_dump(&self, dump: &DumpInfo) -> Result<()> {
        let summary = dump
            .summary
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR REPLACE INTO dumps (id, pid, kind, path, created_at, size_bytes, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                dump.id,
                dump.pid,
                dump.kind,
                dump.path,
                dump.created_at,
                dump.size_bytes as i64,
                summary,
            ],
        )?;
        Ok(())
    }

    pub fn list_dumps(&self) -> Result<Vec<DumpInfo>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, pid, kind, path, created_at, size_bytes, summary
             FROM dumps ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let summary: Option<String> = row.get(6)?;
            Ok(DumpInfo {
                id: row.get(0)?,
                pid: row.get(1)?,
                kind: row.get(2)?,
                path: row.get(3)?,
                created_at: row.get(4)?,
                size_bytes: row.get::<_, i64>(5)? as u64,
                summary: summary.and_then(|text| serde_json::from_str(&text).ok()),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete_dump(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute("DELETE FROM dumps WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn update_dump_summary_by_path(
        &self,
        path: &str,
        summary: &serde_json::Value,
    ) -> Result<()> {
        let text = serde_json::to_string(summary)?;
        let connection = self.connection()?;
        connection.execute(
            "UPDATE dumps SET summary = ?2 WHERE path = ?1",
            params![path, text],
        )?;
        Ok(())
    }

    pub fn list_spring_connections(&self) -> Result<Vec<SpringConnection>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, base_url, username, created_at, last_checked_at
             FROM spring_connections ORDER BY name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SpringConnection {
                id: row.get(0)?,
                name: row.get(1)?,
                base_url: row.get(2)?,
                username: row.get(3)?,
                created_at: row.get(4)?,
                last_checked_at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Saving the same base URL again updates the label and credentials.
    /// Passing `None` for the password keeps the stored secret, which lets the
    /// UI edit a connection without ever sending the old password back.
    pub fn save_spring_connection(
        &self,
        name: &str,
        base_url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<SpringConnection> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO spring_connections (name, base_url, username, password, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(base_url) DO UPDATE SET
                 name = excluded.name,
                 username = excluded.username,
                 password = COALESCE(excluded.password, spring_connections.password)",
            params![name, base_url, username, password, now_ms()],
        )?;
        connection
            .query_row(
                "SELECT id, name, base_url, username, created_at, last_checked_at
                 FROM spring_connections WHERE base_url = ?1",
                params![base_url],
                |row| {
                    Ok(SpringConnection {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        base_url: row.get(2)?,
                        username: row.get(3)?,
                        created_at: row.get(4)?,
                        last_checked_at: row.get(5)?,
                    })
                },
            )
            .map_err(JccError::from)
    }

    pub fn delete_spring_connection(&self, id: i64) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM spring_connections WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn spring_password(&self, id: i64) -> Result<Option<String>> {
        let connection = self.connection()?;
        let password = connection
            .query_row(
                "SELECT password FROM spring_connections WHERE id = ?1",
                params![id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        match password {
            Some(password) => Ok(password),
            None => Err(JccError::NotFound(format!(
                "Spring connection {id} does not exist"
            ))),
        }
    }

    pub fn mark_spring_checked(&self, id: i64, timestamp: i64) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE spring_connections SET last_checked_at = ?2 WHERE id = ?1",
            params![id, timestamp],
        )?;
        Ok(())
    }

    pub fn settings(&self) -> Result<AppSettings> {
        let connection = self.connection()?;
        let value: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'app'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let value = serde_json::to_string(settings)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![value],
        )?;
        Ok(())
    }

    /// Keep the database bounded without touching user-created heap dumps.
    pub fn prune(&self, retention_days: u32, sample_retention_minutes: u32) -> Result<()> {
        let now = now_ms();
        let sample_cutoff = now - i64::from(sample_retention_minutes.max(1)) * 60_000;
        let event_cutoff = now - i64::from(retention_days.max(1)) * 86_400_000;
        let connection = self.connection()?;
        connection.execute("DELETE FROM samples WHERE ts < ?1", params![sample_cutoff])?;
        connection.execute("DELETE FROM gc_events WHERE ts < ?1", params![event_cutoff])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::MonitorSample;

    fn memory_storage() -> Storage {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        connection
            .execute_batch(
                "CREATE TABLE samples (
                    pid INTEGER NOT NULL, ts INTEGER NOT NULL, cpu_percent REAL NOT NULL,
                    heap_used INTEGER NOT NULL, heap_committed INTEGER NOT NULL,
                    heap_max INTEGER NOT NULL, non_heap_used INTEGER NOT NULL,
                    metaspace_used INTEGER NOT NULL, code_cache_used INTEGER NOT NULL,
                    threads_live INTEGER NOT NULL, classes_current INTEGER NOT NULL,
                    gc_count INTEGER NOT NULL, gc_time_ms INTEGER NOT NULL,
                    jit_compiles INTEGER NOT NULL, jit_time_ms INTEGER NOT NULL,
                    PRIMARY KEY (pid, ts)
                );
                CREATE TABLE gc_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, pid INTEGER NOT NULL,
                    ts INTEGER NOT NULL, collector TEXT NOT NULL,
                    invocations INTEGER NOT NULL, duration_ms INTEGER NOT NULL,
                    heap_before INTEGER NOT NULL, heap_after INTEGER NOT NULL,
                    reclaimed_bytes INTEGER NOT NULL, cause TEXT
                );
                CREATE TABLE dumps (
                    id TEXT PRIMARY KEY, pid INTEGER, kind TEXT NOT NULL, path TEXT NOT NULL,
                    created_at INTEGER NOT NULL, size_bytes INTEGER NOT NULL, summary TEXT
                );
                CREATE TABLE spring_connections (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                    base_url TEXT NOT NULL UNIQUE, username TEXT, password TEXT,
                    created_at INTEGER NOT NULL, last_checked_at INTEGER
                );
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .expect("create schema");
        Storage {
            connection: Mutex::new(connection),
        }
    }

    #[test]
    fn samples_round_trip_in_time_order() {
        let storage = memory_storage();
        for ts in [300, 100, 200] {
            storage
                .insert_sample(&MonitorSample {
                    pid: 42,
                    timestamp: ts,
                    heap_used: ts as u64,
                    threads_live: 7,
                    ..Default::default()
                })
                .expect("insert sample");
        }
        let samples = storage.samples(42, 0, 10).expect("read samples");
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].timestamp, 100);
        assert_eq!(samples[2].heap_used, 300);
    }

    #[test]
    fn settings_default_and_round_trip() {
        let storage = memory_storage();
        let defaults = storage.settings().expect("default settings");
        assert_eq!(defaults.refresh_interval_ms, 2_000);
        let updated = AppSettings {
            refresh_interval_ms: 5_000,
            ..defaults
        };
        storage.save_settings(&updated).expect("save settings");
        assert_eq!(
            storage.settings().expect("read settings").refresh_interval_ms,
            5_000
        );
    }

    #[test]
    fn spring_password_survives_update() {
        let storage = memory_storage();
        let connection = storage.save_spring_connection(
            "Orders",
            "http://localhost:8080",
            Some("admin"),
            Some("secret"),
        );
        let connection = connection.expect("save connection");
        storage
            .save_spring_connection("Orders 2", "http://localhost:8080", Some("admin"), None)
            .expect("update connection");
        assert_eq!(
            storage.spring_password(connection.id).expect("password"),
            Some("secret".to_string())
        );
        assert_eq!(storage.list_spring_connections().expect("list").len(), 1);
    }
}
