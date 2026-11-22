use crate::core::error::{JccError, Result};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Tools that JCC can use when they are present in a JDK installation.
pub const JDK_TOOLS: &[&str] = &[
    "java", "javac", "jcmd", "jstat", "jmap", "jstack", "jfr", "jps", "jshell", "jar", "jlink",
    "jconsole", "jhsdb",
];

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A short, deterministic identifier for paths (FNV-1a, 64 bit).
pub fn stable_id(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.to_ascii_lowercase().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// Runs a program with a hard timeout. Stdout and stderr are drained on
/// separate threads so a chatty tool can never fill the OS pipe buffer and
/// deadlock the UI.
pub fn run_command(program: &Path, args: &[String], timeout_ms: u64) -> Result<CommandOutput> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);

    let mut child = command.spawn().map_err(|err| {
        JccError::tool(format!(
            "could not run {}: {err}",
            program.to_string_lossy()
        ))
    })?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| JccError::tool("could not capture tool stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| JccError::tool("could not capture tool stderr"))?;

    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(JccError::Timeout(
                        timeout_ms,
                        program.to_string_lossy().to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(JccError::tool(format!("waiting for tool failed: {err}")));
            }
        }
    };

    let out_bytes = out_reader.join().unwrap_or_default();
    let err_bytes = err_reader.join().unwrap_or_default();

    Ok(CommandOutput {
        code: status.and_then(|s| s.code()),
        stdout: String::from_utf8_lossy(&out_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&err_bytes).into_owned(),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// jcmd prefixes almost every response with `<pid>:` on its own line.
pub fn strip_pid_header(text: &str, pid: u32) -> String {
    let mut lines = text.lines();
    if let Some(first) = lines.next() {
        let trimmed = first.trim();
        if trimmed == format!("{pid}:") || trimmed == format!("{pid}") {
            return lines.collect::<Vec<_>>().join("\n");
        }
    }
    text.to_string()
}

pub fn run_jcmd(
    jcmd: &Path,
    pid: u32,
    command: &str,
    extra: &[String],
    timeout_ms: u64,
) -> Result<String> {
    let mut args = vec![pid.to_string(), command.to_string()];
    args.extend(extra.iter().cloned());
    let output = run_command(jcmd, &args, timeout_ms)?;
    let stdout = strip_pid_header(&output.stdout, pid);
    if output.code.unwrap_or(0) != 0 {
        let detail = if !output.stderr.trim().is_empty() {
            output.stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("{command} failed with exit code {:?}", output.code)
        };
        return Err(JccError::attach(format!(
            "{command} failed for PID {pid}: {detail}"
        )));
    }
    if stdout.trim().is_empty() && !output.stderr.trim().is_empty() {
        // Some JDK versions report attach problems on stderr while exiting 0.
        let stderr = output.stderr.trim();
        if stderr.contains("Exception") || stderr.contains("Error") {
            return Err(JccError::attach(format!(
                "{command} failed for PID {pid}: {stderr}"
            )));
        }
    }
    Ok(stdout)
}

pub fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(executable_name(name));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn jcmd_for_home(java_home: &Path) -> Option<PathBuf> {
    let candidate = java_home.join("bin").join(executable_name("jcmd"));
    candidate.is_file().then_some(candidate)
}

/// Resolve the JDK home that owns a running `java` executable.
pub fn java_home_from_executable(executable: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    let mut current = resolved.parent()?;
    for _ in 0..4 {
        if current.join("bin").join(executable_name("java")).is_file()
            && (current.join("release").is_file() || current.join("lib").is_dir())
        {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
    None
}

pub fn read_release_file(java_home: &Path) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(java_home.join("release")) else {
        return values;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        values.insert(key.trim().to_string(), value);
    }
    values
}

pub fn parse_java_version_output(text: &str) -> Option<(String, u32)> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("version \"") || lower.contains("openjdk ")) {
            continue;
        }
        let start = line.find('"')?;
        let rest = &line[start + 1..];
        let end = rest.find('"')?;
        let version = rest[..end].to_string();
        if let Some(major) = version_major(&version) {
            return Some((version, major));
        }
    }
    None
}

pub fn version_major(version: &str) -> Option<u32> {
    let cleaned = version.trim().trim_start_matches("v");
    let mut parts = cleaned.split(['.', '+', '-']);
    let first = parts.next()?;
    let first_num: u32 = first.parse().ok()?;
    if first_num == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first_num)
    }
}

pub fn java_home_from_env() -> Option<PathBuf> {
    let home = std::env::var_os("JAVA_HOME")?;
    let path = PathBuf::from(home);
    let java = path.join("bin").join(executable_name("java"));
    java.is_file().then_some(path)
}

pub fn default_java_home() -> Option<PathBuf> {
    if let Some(home) = java_home_from_env() {
        return Some(home);
    }
    let java = find_in_path("java")?;
    java_home_from_executable(&java)
}

pub fn open_in_file_manager(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(JccError::NotFound(format!(
            "{} does not exist",
            path.display()
        )));
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer.exe");
        if path.is_file() {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        hide_console(&mut command);
        command
            .spawn()
            .map_err(|err| JccError::msg(format!("could not open Explorer: {err}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        if path.is_file() {
            command.arg("-R");
        }
        command
            .arg(path)
            .spawn()
            .map_err(|err| JccError::msg(format!("could not open Finder: {err}")))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|err| JccError::msg(format!("could not open file manager: {err}")))?;
    }
    Ok(())
}

pub fn human_os() -> String {
    std::env::consts::OS.to_string()
}

pub fn executable_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_pid_header_only_once() {
        assert_eq!(strip_pid_header("123:\nhello\nworld", 123), "hello\nworld");
        assert_eq!(strip_pid_header("hello\nworld", 123), "hello\nworld");
        assert_eq!(strip_pid_header("1234:\nhello", 123), "1234:\nhello");
    }

    #[test]
    fn parses_modern_and_legacy_versions() {
        assert_eq!(
            parse_java_version_output("openjdk version \"21.0.9\" 2025-10-21"),
            Some(("21.0.9".to_string(), 21))
        );
        assert_eq!(
            parse_java_version_output("java version \"1.8.0_392\""),
            Some(("1.8.0_392".to_string(), 8))
        );
        assert_eq!(version_major("17-ea"), Some(17));
        assert_eq!(version_major("1.8.0_402"), Some(8));
    }

    #[test]
    fn ids_are_stable_and_case_insensitive() {
        assert_eq!(stable_id("C:\\Java\\JDK"), stable_id("c:\\java\\jdk"));
        assert_eq!(stable_id("abc").len(), 16);
    }

    #[test]
    fn reads_release_key_values() {
        let dir = std::env::temp_dir().join(format!("jcc-release-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("release"),
            "JAVA_VERSION=\"21.0.9\"\nIMPLEMENTOR=\"Oracle Corporation\"\n",
        )
        .unwrap();
        let values = read_release_file(&dir);
        assert_eq!(values.get("JAVA_VERSION"), Some(&"21.0.9".to_string()));
        assert_eq!(
            values.get("IMPLEMENTOR"),
            Some(&"Oracle Corporation".to_string())
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
