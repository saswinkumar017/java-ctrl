use crate::core::error::{JccError, Result};
use crate::core::models::{JfrRecording, RuntimeDetails};
use crate::core::tools::{now_ms, run_jcmd};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLineDetails {
    pub jvm_args: Vec<String>,
    pub java_command: Option<String>,
    pub class_path: Option<String>,
    pub launcher_type: Option<String>,
}

pub fn parse_system_properties(text: &str) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim().is_empty() {
            continue;
        }
        properties.insert(key.trim().to_string(), value.to_string());
    }
    properties
}

pub fn parse_command_line(text: &str) -> CommandLineDetails {
    let mut details = CommandLineDetails::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("jvm_args:") {
            details.jvm_args = value.split_whitespace().map(ToString::to_string).collect();
        } else if let Some(value) = trimmed.strip_prefix("java_command:") {
            let value = value.trim();
            if !value.is_empty() {
                details.java_command = Some(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix("java_class_path (initial):") {
            let value = value.trim();
            if !value.is_empty() {
                details.class_path = Some(value.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix("Launcher Type:") {
            let value = value.trim();
            if !value.is_empty() {
                details.launcher_type = Some(value.to_string());
            }
        }
    }
    details
}

pub fn parse_jfr_check(text: &str) -> Vec<JfrRecording> {
    let mut recordings = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Recording ") {
            continue;
        }
        let Some((id_text, rest)) = trimmed
            .trim_start_matches("Recording ")
            .split_once(':')
        else {
            continue;
        };
        let Ok(id) = id_text.trim().parse::<u32>() else {
            continue;
        };
        let mut recording = JfrRecording {
            id,
            name: None,
            state: None,
            duration: None,
            destination: None,
            raw: trimmed.to_string(),
        };
        let mut body = rest.trim();
        if let Some(open) = body.rfind('(') {
            if body.ends_with(')') {
                recording.state = Some(body[open + 1..body.len() - 1].trim().to_string());
                body = body[..open].trim();
            }
        }
        for token in body.split_whitespace() {
            let Some((key, value)) = token.split_once('=') else {
                continue;
            };
            let value = value.trim_matches('"').to_string();
            match key {
                "name" => recording.name = Some(value),
                "duration" => recording.duration = Some(value),
                "destination" => recording.destination = Some(value),
                "state" => recording.state = Some(value),
                _ => {}
            }
        }
        recordings.push(recording);
    }
    recordings
}

pub fn read_runtime_details(
    jcmd: &Path,
    pid: u32,
    uptime_secs: u64,
) -> Result<RuntimeDetails> {
    let mut notes = Vec::new();
    let properties = match run_jcmd(jcmd, pid, "VM.system_properties", &[], 15_000) {
        Ok(text) => parse_system_properties(&text),
        Err(err) => {
            notes.push(format!("System properties unavailable: {err}"));
            BTreeMap::new()
        }
    };
    let command_line = match run_jcmd(jcmd, pid, "VM.command_line", &[], 10_000) {
        Ok(text) => parse_command_line(&text),
        Err(err) => {
            notes.push(format!("VM command line unavailable: {err}"));
            CommandLineDetails::default()
        }
    };
    let flags = match run_jcmd(jcmd, pid, "VM.flags", &[], 10_000) {
        Ok(text) => text.split_whitespace().map(ToString::to_string).collect(),
        Err(err) => {
            notes.push(format!("VM flags unavailable: {err}"));
            Vec::new()
        }
    };
    let version_lines = match run_jcmd(jcmd, pid, "VM.version", &[], 10_000) {
        Ok(text) => text.lines().map(str::trim).filter(|line| !line.is_empty()).map(ToString::to_string).collect(),
        Err(err) => {
            notes.push(format!("VM version unavailable: {err}"));
            Vec::new()
        }
    };
    let jfr_recordings = match run_jcmd(jcmd, pid, "JFR.check", &[], 10_000) {
        Ok(text) => parse_jfr_check(&text),
        Err(_) => Vec::new(),
    };

    if properties.is_empty() && version_lines.is_empty() {
        return Err(JccError::attach(format!(
            "could not read runtime details from PID {pid}"
        )));
    }

    Ok(RuntimeDetails {
        pid,
        properties,
        jvm_args: command_line.jvm_args,
        java_command: command_line.java_command,
        class_path: command_line.class_path,
        launcher_type: command_line.launcher_type,
        flags,
        version_lines,
        uptime_secs,
        jfr_recordings,
        notes,
    })
}

pub fn start_jfr(
    jcmd: &Path,
    pid: u32,
    name: &str,
    duration_secs: u64,
    filename: Option<&str>,
) -> Result<String> {
    let mut args = vec![
        format!("name={name}"),
        "settings=profile".to_string(),
    ];
    if duration_secs > 0 {
        args.push(format!("duration={duration_secs}s"));
    }
    if let Some(filename) = filename {
        args.push(format!("filename={filename}"));
    }
    run_jcmd(jcmd, pid, "JFR.start", &args, 15_000)
}

pub fn dump_jfr(jcmd: &Path, pid: u32, name: &str, filename: &str) -> Result<String> {
    run_jcmd(
        jcmd,
        pid,
        "JFR.dump",
        &[
            format!("name={name}"),
            format!("filename={filename}"),
        ],
        60_000,
    )
}

pub fn stop_jfr(jcmd: &Path, pid: u32, name: &str) -> Result<String> {
    run_jcmd(jcmd, pid, "JFR.stop", &[format!("name={name}")], 15_000)
}

pub fn jfr_check(jcmd: &Path, pid: u32) -> Result<Vec<JfrRecording>> {
    let text = run_jcmd(jcmd, pid, "JFR.check", &[], 10_000)?;
    Ok(parse_jfr_check(&text))
}

pub fn runtime_timestamp() -> i64 {
    now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_properties_with_equals_in_values() {
        let properties = parse_system_properties(
            "java.version=21.0.9\njava.class.path=C:\\a=b\\classes\nempty=\n# comment\n",
        );
        assert_eq!(properties.get("java.version").map(String::as_str), Some("21.0.9"));
        assert_eq!(
            properties.get("java.class.path").map(String::as_str),
            Some("C:\\a=b\\classes")
        );
        assert_eq!(properties.get("empty").map(String::as_str), Some(""));
        assert!(!properties.contains_key("# comment"));
    }

    #[test]
    fn parses_vm_command_line() {
        let details = parse_command_line(
            "VM Arguments:\njvm_args: -Xmx512m -XX:+UseG1GC \njava_command: com.example.Main\njava_class_path (initial): C:\\work\\classes\nLauncher Type: SUN_STANDARD\n",
        );
        assert_eq!(details.jvm_args, vec!["-Xmx512m", "-XX:+UseG1GC"]);
        assert_eq!(details.java_command.as_deref(), Some("com.example.Main"));
        assert_eq!(details.class_path.as_deref(), Some("C:\\work\\classes"));
        assert_eq!(details.launcher_type.as_deref(), Some("SUN_STANDARD"));
    }

    #[test]
    fn parses_jfr_recordings() {
        let recordings = parse_jfr_check(
            "No available recordings.\n\nRecording 1: name=jcc-test duration=1m (running)\n",
        );
        assert_eq!(recordings.len(), 1);
        assert_eq!(recordings[0].id, 1);
        assert_eq!(recordings[0].name.as_deref(), Some("jcc-test"));
        assert_eq!(recordings[0].duration.as_deref(), Some("1m"));
        assert_eq!(recordings[0].state.as_deref(), Some("running"));
    }
}
