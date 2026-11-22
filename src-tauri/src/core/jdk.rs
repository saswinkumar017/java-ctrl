use crate::core::error::Result;
use crate::core::models::{JdkInfo, JdkScan, JdkTool};
use crate::core::tools::{
    executable_name, find_in_path, java_home_from_env, java_home_from_executable,
    parse_java_version_output, read_release_file, run_command, stable_id, version_major,
    JDK_TOOLS,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Discover JDKs from environment variables, PATH, well-known install roots
/// and (on Windows) the JavaSoft registry keys. Discovery never mutates the
/// system and degrades gracefully when a location is unreadable.
pub fn scan_jdks(default_override: Option<&str>) -> JdkScan {
    let mut notes = Vec::new();
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();

    if let Some(home) = java_home_from_env() {
        candidates.push((home, "JAVA_HOME".to_string()));
    }
    if let Some(java) = find_in_path("java") {
        if let Some(home) = java_home_from_executable(&java) {
            candidates.push((home, "PATH".to_string()));
        }
    }

    #[cfg(target_os = "windows")]
    {
        for home in windows_registry_homes() {
            candidates.push((home, "registry".to_string()));
        }
        for (parent, source) in windows_scan_roots() {
            collect_children(&parent, &source, &mut candidates, 2);
        }
    }

    #[cfg(target_os = "macos")]
    {
        collect_children(
            Path::new("/Library/Java/JavaVirtualMachines"),
            "system",
            &mut candidates,
            2,
        );
        if let Some(home) = dirs::home_dir() {
            collect_children(&home.join("Library/Java/JavaVirtualMachines"), "user", &mut candidates, 2);
            collect_children(&home.join(".sdkman/candidates/java"), "sdkman", &mut candidates, 1);
            collect_children(&home.join(".jdks"), "intellij", &mut candidates, 1);
        }
        collect_children(Path::new("/opt/homebrew/opt"), "homebrew", &mut candidates, 1);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for root in [
            "/usr/lib/jvm",
            "/usr/java",
            "/usr/local/java",
            "/opt/java",
            "/opt/jdk",
        ] {
            collect_children(Path::new(root), "system", &mut candidates, 2);
        }
        if let Some(home) = dirs::home_dir() {
            collect_children(&home.join(".sdkman/candidates/java"), "sdkman", &mut candidates, 1);
            collect_children(&home.join(".asdf/installs/java"), "asdf", &mut candidates, 1);
            collect_children(&home.join(".jdks"), "intellij", &mut candidates, 1);
            collect_children(&home.join(".gradle/jdks"), "gradle", &mut candidates, 1);
            collect_children(&home.join(".local/share/mise/installs/java"), "mise", &mut candidates, 1);
        }
    }

    let env_default = java_home_from_env();
    let path_default = find_in_path("java").and_then(|java| java_home_from_executable(&java));
    let default_home = default_override
        .map(PathBuf::from)
        .filter(|path| is_jdk_home(path))
        .or(env_default)
        .or(path_default);

    let mut seen: Vec<String> = Vec::new();
    let mut jdks: Vec<JdkInfo> = Vec::new();

    for (candidate, source) in candidates {
        let Some(home) = normalize_home(&candidate) else {
            continue;
        };
        let key = path_key(&home);
        if seen.iter().any(|existing| existing == &key) {
            continue;
        }
        seen.push(key);
        if let Some(jdk) = inspect_jdk(&home, &source, default_home.as_deref()) {
            jdks.push(jdk);
        }
    }

    jdks.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| b.version_major.unwrap_or(0).cmp(&a.version_major.unwrap_or(0)))
            .then_with(|| a.java_home.cmp(&b.java_home))
    });

    if jdks.is_empty() {
        notes.push(
            "No JDK installations were found. Set JAVA_HOME or install a JDK, then rescan."
                .to_string(),
        );
    }
    let missing_jcmd = jdks.iter().filter(|jdk| {
        !jdk.tools
            .iter()
            .any(|tool| tool.name == "jcmd" && tool.available)
    });
    if jdks.iter().any(|jdk| jdk.version_major.unwrap_or(0) < 8) {
        notes.push("JDK 7 and older are detected but JCC needs jcmd (JDK 7+) and newer attach APIs for full monitoring.".to_string());
    }
    if missing_jcmd.count() > 0 {
        notes.push(
            "Some installations do not include jcmd. JCC can list them but cannot attach for deep JVM data."
                .to_string(),
        );
    }

    JdkScan {
        jdks,
        default_java_home: default_home.map(|path| path.to_string_lossy().to_string()),
        scanned_at: crate::core::tools::now_ms(),
        notes,
    }
}

pub fn is_jdk_home(path: &Path) -> bool {
    path.join("bin")
        .join(executable_name("java"))
        .is_file()
        && (path.join("release").is_file() || path.join("lib").is_dir())
}

fn normalize_home(path: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(path).ok()?;
    // macOS bundle layouts expose the real home under Contents/Home.
    if resolved.join("Contents/Home/bin").join(executable_name("java")).is_file() {
        return std::fs::canonicalize(resolved.join("Contents/Home")).ok();
    }
    is_jdk_home(&resolved).then_some(resolved)
}

fn path_key(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

fn collect_children(
    parent: &Path,
    source: &str,
    candidates: &mut Vec<(PathBuf, String)>,
    depth: u8,
) {
    if !parent.is_dir() {
        return;
    }
    if is_jdk_home(parent) {
        candidates.push((parent.to_path_buf(), source.to_string()));
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_jdk_home(&path) {
            candidates.push((path, source.to_string()));
        } else if depth > 1 {
            collect_children(&path, source, candidates, depth - 1);
        }
    }
}

fn inspect_jdk(home: &Path, source: &str, default_home: Option<&Path>) -> Option<JdkInfo> {
    let java = home.join("bin").join(executable_name("java"));
    if !java.is_file() {
        return None;
    }
    let release = read_release_file(home);
    let mut version = release.get("JAVA_VERSION").cloned();
    let mut major = version.as_deref().and_then(version_major);
    let mut vendor = release
        .get("IMPLEMENTOR")
        .cloned()
        .or_else(|| release.get("JAVA_VENDOR").cloned());
    let mut vm_name = release.get("JAVA_VM").cloned();

    if version.is_none() || major.is_none() || vendor.is_none() {
        if let Ok(output) = run_command(&java, &["-version".to_string()], 5_000) {
            let combined = format!("{}\n{}", output.stderr, output.stdout);
            if let Some((parsed, parsed_major)) = parse_java_version_output(&combined) {
                version = version.or(Some(parsed));
                major = major.or(Some(parsed_major));
            }
            if vendor.is_none() {
                vendor = combined
                    .lines()
                    .next()
                    .and_then(|line| line.split_once(' '))
                    .map(|(name, _)| name.to_string());
            }
            if vm_name.is_none() {
                vm_name = combined
                    .lines()
                    .find(|line| line.contains("VM"))
                    .map(|line| line.trim().to_string());
            }
        }
    }

    let tools = JDK_TOOLS
        .iter()
        .map(|name| {
            let candidate = home.join("bin").join(executable_name(name));
            JdkTool {
                name: (*name).to_string(),
                path: candidate
                    .is_file()
                    .then(|| candidate.to_string_lossy().to_string()),
                available: candidate.is_file(),
            }
        })
        .collect::<Vec<_>>();

    let javac = home.join("bin").join(executable_name("javac"));
    let java_home_text = home.to_string_lossy().to_string();
    let is_default = default_home
        .map(|default| path_key(default) == path_key(home))
        .unwrap_or(false);

    Some(JdkInfo {
        id: stable_id(&java_home_text),
        java_home: java_home_text,
        java_executable: java.to_string_lossy().to_string(),
        javac_executable: javac.is_file().then(|| javac.to_string_lossy().to_string()),
        version,
        version_major: major,
        vendor,
        vm_name,
        architecture: release.get("OS_ARCH").cloned(),
        os_arch: release.get("OS_ARCH").cloned(),
        source: source.to_string(),
        is_default,
        tools,
    })
}

#[cfg(target_os = "windows")]
fn windows_scan_roots() -> Vec<(PathBuf, String)> {
    let mut roots = Vec::new();
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let program_files_x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let home = dirs::home_dir();

    for base in [program_files, program_files_x86].into_iter().flatten() {
        for vendor in [
            "Java",
            "Eclipse Adoptium",
            "Eclipse Foundation",
            "AdoptOpenJDK",
            "Microsoft",
            "Amazon Corretto",
            "Zulu",
            "BellSoft",
            "IBM",
            "Semeru",
            "RedHat",
            "SapMachine",
            "JetBrains",
            "GraalVM",
            "OpenJDK",
        ] {
            roots.push((base.join(vendor), "system".to_string()));
        }
        roots.push((base.clone(), "system".to_string()));
    }
    if let Some(base) = local_app_data {
        for vendor in [
            "Programs/Eclipse Adoptium",
            "Programs/Microsoft",
            "Programs/Zulu",
            "Programs/BellSoft",
            "Programs/Amazon Corretto",
            "Programs/Java",
        ] {
            roots.push((base.join(vendor), "user".to_string()));
        }
    }
    if let Some(home) = home {
        roots.push((home.join(".jdks"), "intellij".to_string()));
        roots.push((home.join(".sdkman/candidates/java"), "sdkman".to_string()));
        roots.push((home.join(".gradle/jdks"), "gradle".to_string()));
        roots.push((home.join(".asdf/installs/java"), "asdf".to_string()));
    }
    roots
}

#[cfg(target_os = "windows")]
fn windows_registry_homes() -> Vec<PathBuf> {
    use crate::core::tools::run_command;
    let keys = [
        r"HKLM\SOFTWARE\JavaSoft\JDK",
        r"HKLM\SOFTWARE\JavaSoft\Java Development Kit",
        r"HKLM\SOFTWARE\JavaSoft\JRE",
        r"HKLM\SOFTWARE\JavaSoft\Java Runtime Environment",
        r"HKLM\SOFTWARE\WOW6432Node\JavaSoft\JDK",
        r"HKLM\SOFTWARE\Eclipse Adoptium\JDK",
        r"HKLM\SOFTWARE\Microsoft\JDK",
    ];
    let mut homes = Vec::new();
    for key in keys {
        let args = vec![
            "query".to_string(),
            key.to_string(),
            "/s".to_string(),
            "/v".to_string(),
            "JavaHome".to_string(),
        ];
        let Ok(output) = run_command(Path::new("reg.exe"), &args, 4_000) else {
            continue;
        };
        for line in output.stdout.lines() {
            let Some(index) = line.to_ascii_uppercase().find("REG_SZ") else {
                continue;
            };
            let value = line[index + "REG_SZ".len()..].trim();
            if !value.is_empty() {
                homes.push(PathBuf::from(value));
            }
        }
    }
    homes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_jdk_directories() {
        assert!(!is_jdk_home(Path::new("C:\\definitely-not-a-jdk")));
    }

    #[test]
    fn scans_without_panicking() {
        let scan = scan_jdks(None);
        assert!(scan.scanned_at > 0);
        for jdk in &scan.jdks {
            assert!(!jdk.id.is_empty());
            assert!(Path::new(&jdk.java_home).is_absolute());
        }
    }
}
