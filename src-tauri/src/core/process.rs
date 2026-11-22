use crate::core::error::{JccError, Result};
use crate::core::jdk::is_jdk_home;
use crate::core::models::{JvmProcess, TerminationResult};
use crate::core::tools::{
    default_java_home, executable_stem, java_home_from_executable, jcmd_for_home, read_release_file,
    run_command, run_jcmd,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Default)]
pub struct OsMetrics {
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
    pub uptime_secs: u64,
    pub start_time: u64,
}

pub struct ProcessService {
    system: System,
}

impl ProcessService {
    pub fn new() -> Result<Self> {
        let system = System::new_all()
            .map_err(|err| JccError::msg(format!("could not read the process table: {err}")))?;
        Ok(Self { system })
    }

    pub fn refresh_all(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
    }

    pub fn refresh_one(&mut self, pid: u32) -> bool {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            false,
            ProcessRefreshKind::everything(),
        ) > 0
    }

    pub fn list(&mut self) -> Vec<JvmProcess> {
        self.refresh_all();
        let mut processes = self
            .system
            .processes()
            .iter()
            .filter_map(|(pid, process)| build_process(pid.as_u32(), process))
            .collect::<Vec<_>>();
        processes.sort_by_key(|process| std::cmp::Reverse(process.start_time));
        processes
    }

    pub fn get(&mut self, pid: u32) -> Option<JvmProcess> {
        self.refresh_one(pid);
        self.system
            .process(Pid::from_u32(pid))
            .and_then(|process| build_process(pid, process))
    }

    pub fn os_metrics(&mut self, pid: u32) -> Option<OsMetrics> {
        if !self.refresh_one(pid) {
            return None;
        }
        let process = self.system.process(Pid::from_u32(pid))?;
        Some(OsMetrics {
            cpu_percent: process.cpu_usage(),
            rss_bytes: process.memory(),
            virtual_bytes: process.virtual_memory(),
            uptime_secs: process.run_time(),
            start_time: process.start_time(),
        })
    }

    pub fn is_java(&mut self, pid: u32) -> bool {
        self.refresh_one(pid);
        self.system
            .process(Pid::from_u32(pid))
            .map(|process| is_java_process(process))
            .unwrap_or(false)
    }

    pub fn exists(&mut self, pid: u32) -> bool {
        self.refresh_one(pid);
        self.system.process(Pid::from_u32(pid)).is_some()
    }
}

fn build_process(pid: u32, process: &sysinfo::Process) -> Option<JvmProcess> {
    if !is_java_process(process) {
        return None;
    }
    let args = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let executable = process.exe().map(|path| path.to_string_lossy().to_string());
    let parsed = parse_java_command(&args);
    let java_home = executable
        .as_deref()
        .and_then(|path| java_home_from_executable(Path::new(path)))
        .filter(|path| is_jdk_home(path));
    let release = java_home.as_deref().map(read_release_file).unwrap_or_default();
    let version = release.get("JAVA_VERSION").cloned();
    let vendor = release.get("IMPLEMENTOR").cloned();
    let attachable = java_home
        .as_deref()
        .and_then(jcmd_for_home)
        .is_some()
        || default_java_home().and_then(|home| jcmd_for_home(&home)).is_some();
    let spring_ports = detect_spring_ports(&args, parsed.main_jar.as_deref());

    Some(JvmProcess {
        pid,
        name: process.name().to_string_lossy().to_string(),
        display_name: parsed.display_name.clone(),
        main_class: parsed.main_class,
        main_jar: parsed.main_jar,
        java_home: java_home.map(|path| path.to_string_lossy().to_string()),
        java_version: version,
        vendor,
        command_line: args.join(" "),
        args,
        jvm_args: parsed.jvm_args,
        app_args: parsed.app_args,
        uptime_secs: process.run_time(),
        start_time: process.start_time(),
        cpu_percent: process.cpu_usage(),
        rss_bytes: process.memory(),
        virtual_bytes: process.virtual_memory(),
        executable,
        cwd: process.cwd().map(|path| path.to_string_lossy().to_string()),
        spring_ports,
        attachable,
    })
}

pub fn is_java_process(process: &sysinfo::Process) -> bool {
    let name = process.name().to_string_lossy().to_ascii_lowercase();
    if name == "java" || name == "javaw" || name == "java.exe" || name == "javaw.exe" {
        return true;
    }
    process
        .exe()
        .map(|path| matches!(executable_stem(path).as_str(), "java" | "javaw"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedJavaCommand {
    pub main_class: Option<String>,
    pub main_jar: Option<String>,
    pub display_name: String,
    pub jvm_args: Vec<String>,
    pub app_args: Vec<String>,
}

/// Parse the launcher portion of a Java command line without trying to model
/// every HotSpot option. Options that take a value (`-cp`, `-jar`, `-m`) are
/// handled explicitly so the main class is never mistaken for a value.
pub fn parse_java_command(args: &[String]) -> ParsedJavaCommand {
    let mut parsed = ParsedJavaCommand {
        display_name: "Java process".to_string(),
        ..Default::default()
    };
    let mut index = 0;
    if args
        .first()
        .map(|arg| {
            let lower = arg.to_ascii_lowercase();
            lower.ends_with("java") || lower.ends_with("java.exe") || lower.ends_with("javaw.exe")
        })
        .unwrap_or(false)
    {
        index = 1;
    }

    while index < args.len() {
        let arg = &args[index];
        if arg == "-jar" {
            parsed.jvm_args.push(arg.clone());
            if let Some(jar) = args.get(index + 1) {
                parsed.main_jar = Some(jar.clone());
                parsed.display_name = jar_display_name(jar);
                parsed.jvm_args.push(jar.clone());
                parsed.app_args = args[index + 2..].to_vec();
            }
            return parsed;
        }
        if arg == "-m" || arg == "--module" {
            parsed.jvm_args.push(arg.clone());
            if let Some(module) = args.get(index + 1) {
                parsed.main_class = Some(module.clone());
                parsed.display_name = main_display_name(module);
                parsed.jvm_args.push(module.clone());
                parsed.app_args = args[index + 2..].to_vec();
            }
            return parsed;
        }
        if let Some(module) = arg.strip_prefix("--module=") {
            parsed.jvm_args.push(arg.clone());
            parsed.main_class = Some(module.to_string());
            parsed.display_name = main_display_name(module);
            parsed.app_args = args[index + 1..].to_vec();
            return parsed;
        }
        if matches!(
            arg.as_str(),
            "-cp" | "-classpath" | "--class-path" | "-p" | "--module-path" | "--upgrade-module-path"
        ) {
            parsed.jvm_args.push(arg.clone());
            if let Some(value) = args.get(index + 1) {
                parsed.jvm_args.push(value.clone());
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        if arg.starts_with("--class-path=")
            || arg.starts_with("--module-path=")
            || arg.starts_with("-D")
            || arg.starts_with("-X")
            || arg.starts_with("-agent")
            || arg.starts_with("-javaagent")
            || arg.starts_with("-verbose")
            || arg.starts_with("-splash")
            || arg.starts_with("-esa")
            || arg.starts_with("-dsa")
            || arg.starts_with("-server")
            || arg.starts_with("-client")
        {
            parsed.jvm_args.push(arg.clone());
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            parsed.jvm_args.push(arg.clone());
            index += 1;
            continue;
        }
        parsed.main_class = Some(arg.clone());
        parsed.display_name = main_display_name(arg);
        parsed.app_args = args[index + 1..].to_vec();
        return parsed;
    }
    parsed
}

fn jar_display_name(jar: &str) -> String {
    Path::new(jar)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| jar.to_string())
}

fn main_display_name(main: &str) -> String {
    let last = main.rsplit(['.', '/']).next().unwrap_or(main);
    if last.is_empty() {
        main.to_string()
    } else {
        last.to_string()
    }
}

pub fn detect_spring_ports(args: &[String], main_jar: Option<&str>) -> Vec<u16> {
    let mut ports = Vec::new();
    let mut push = |value: &str| {
        if let Ok(port) = value.parse::<u16>() {
            if port > 0 && !ports.contains(&port) {
                ports.push(port);
            }
        }
    };
    for (index, arg) in args.iter().enumerate() {
        let value = if let Some(value) = arg.strip_prefix("--server.port=") {
            Some(value)
        } else if let Some(value) = arg.strip_prefix("--management.server.port=") {
            Some(value)
        } else if let Some(value) = arg.strip_prefix("-Dserver.port=") {
            Some(value)
        } else if let Some(value) = arg.strip_prefix("-Dmanagement.server.port=") {
            Some(value)
        } else if matches!(
            arg.as_str(),
            "--server.port" | "--management.server.port" | "-Dserver.port" | "-Dmanagement.server.port"
        ) {
            args.get(index + 1).map(String::as_str)
        } else {
            None
        };
        if let Some(value) = value {
            push(value);
        }
    }
    if ports.is_empty() {
        let spring_marker = main_jar
            .map(|jar| {
                let lower = jar.to_ascii_lowercase();
                lower.contains("spring-boot") || lower.contains("springboot")
            })
            .unwrap_or(false)
            || args.iter().any(|arg| {
                let lower = arg.to_ascii_lowercase();
                lower.contains("spring-boot") || lower.contains("spring.profiles")
            });
        if spring_marker {
            ports.push(8080);
        }
    }
    ports
}

/// Graceful shutdown asks the target JVM to run its shutdown hooks via jcmd.
/// Force shutdown uses the OS; both verify that the target is actually a Java
/// process first so a stale PID can never kill an unrelated application.
pub fn terminate_process(
    process_service: &mut ProcessService,
    jcmd: Option<&Path>,
    pid: u32,
    force: bool,
) -> Result<TerminationResult> {
    if pid <= 4 {
        return Err(JccError::invalid("refusing to terminate a system process"));
    }
    if pid == std::process::id() {
        return Err(JccError::invalid("refusing to terminate Java Control Centre"));
    }
    if !process_service.exists(pid) {
        return Err(JccError::NotFound(format!("process {pid} is not running")));
    }
    if !process_service.is_java(pid) {
        return Err(JccError::invalid(format!(
            "process {pid} is not a Java process; refusing to terminate it"
        )));
    }

    if force {
        let mut detail = String::new();
        #[cfg(windows)]
        {
            let args = vec![
                "/PID".to_string(),
                pid.to_string(),
                "/F".to_string(),
                "/T".to_string(),
            ];
            if let Ok(output) = run_command(Path::new("taskkill.exe"), &args, 15_000) {
                detail = format!("{}{}", output.stdout, output.stderr).trim().to_string();
            }
        }
        #[cfg(unix)]
        {
            let args = vec!["-9".to_string(), pid.to_string()];
            if let Ok(output) = run_command(Path::new("kill"), &args, 10_000) {
                detail = format!("{}{}", output.stdout, output.stderr).trim().to_string();
            }
        }
        let exited = wait_for_exit(process_service, pid, 8_000);
        return Ok(TerminationResult {
            pid,
            force: true,
            status: if exited { "terminated" } else { "still-running" }.to_string(),
            message: if exited {
                "Process was force terminated.".to_string()
            } else {
                format!(
                    "The process did not exit after a force terminate request.{}",
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" OS said: {detail}")
                    }
                )
            },
        });
    }

    let Some(jcmd) = jcmd else {
        return Ok(TerminationResult {
            pid,
            force: false,
            status: "unavailable".to_string(),
            message: "Graceful shutdown needs jcmd from a JDK. Install a full JDK or use force terminate."
                .to_string(),
        });
    };

    let _ = run_jcmd(jcmd, pid, "VM.shutdown", &[], 10_000);
    let exited = wait_for_exit(process_service, pid, 8_000);
    Ok(TerminationResult {
        pid,
        force: false,
        status: if exited { "terminated" } else { "still-running" }.to_string(),
        message: if exited {
            "Shutdown hooks completed and the JVM exited.".to_string()
        } else {
            "The JVM is still running. It may be ignoring shutdown, or shutdown hooks may be blocked."
                .to_string()
        },
    })
}

fn wait_for_exit(process_service: &mut ProcessService, pid: u32, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if !process_service.exists(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    !process_service.exists(pid)
}

pub fn jcmd_for_process(process: &JvmProcess) -> Option<PathBuf> {
    process
        .java_home
        .as_deref()
        .and_then(|home| jcmd_for_home(Path::new(home)))
        .or_else(|| default_java_home().and_then(|home| jcmd_for_home(&home)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jar_launch() {
        let args = vec![
            "java".to_string(),
            "-Xmx2g".to_string(),
            "-Dserver.port=9090".to_string(),
            "-jar".to_string(),
            "C:\\apps\\orders-service.jar".to_string(),
            "--spring.profiles.active=prod".to_string(),
        ];
        let parsed = parse_java_command(&args);
        assert_eq!(parsed.main_jar.as_deref(), Some("C:\\apps\\orders-service.jar"));
        assert_eq!(parsed.display_name, "orders-service");
        assert_eq!(parsed.jvm_args.len(), 4);
        assert_eq!(parsed.app_args.len(), 1);
        assert_eq!(detect_spring_ports(&args, parsed.main_jar.as_deref()), vec![9090]);
    }

    #[test]
    fn parses_classpath_launch() {
        let args = vec![
            "java".to_string(),
            "-cp".to_string(),
            "build/classes".to_string(),
            "com.example.Main".to_string(),
            "--port".to_string(),
            "7000".to_string(),
        ];
        let parsed = parse_java_command(&args);
        assert_eq!(parsed.main_class.as_deref(), Some("com.example.Main"));
        assert_eq!(parsed.display_name, "Main");
        assert_eq!(parsed.app_args, vec!["--port", "7000"]);
    }

    #[test]
    fn parses_module_launch() {
        let args = vec![
            "java".to_string(),
            "-m".to_string(),
            "com.example.app/com.example.Main".to_string(),
        ];
        let parsed = parse_java_command(&args);
        assert_eq!(
            parsed.main_class.as_deref(),
            Some("com.example.app/com.example.Main")
        );
        assert_eq!(parsed.display_name, "Main");
    }

    #[test]
    fn detects_spring_default_port() {
        assert_eq!(
            detect_spring_ports(
                &["java".to_string(), "-jar".to_string(), "my-spring-boot-app.jar".to_string()],
                Some("my-spring-boot-app.jar")
            ),
            vec![8080]
        );
        assert!(detect_spring_ports(&["java".to_string(), "-jar".to_string(), "plain.jar".to_string()], Some("plain.jar")).is_empty());
    }
}
