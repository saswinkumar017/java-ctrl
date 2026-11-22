use crate::core::error::{JccError, Result};
use crate::core::models::{ClassHistogramRow, ClassLoaderInfo, ClassLoadingSnapshot};
use crate::core::tools::{now_ms, run_jcmd};
use std::path::Path;

pub struct ClassLoaderStats {
    pub loaders: Vec<ClassLoaderInfo>,
    pub total_loaders: u64,
    pub total_classes: u64,
    pub total_chunks: u64,
    pub total_blocks: u64,
}

pub fn parse_classloader_stats(text: &str) -> ClassLoaderStats {
    let mut loaders: Vec<ClassLoaderInfo> = Vec::new();
    let mut total_loaders = 0;
    let mut total_classes = 0;
    let mut total_chunks = 0;
    let mut total_blocks = 0;
    let mut previous: Option<usize> = None;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("ClassLoader") || trimmed.starts_with("ChunkSz")
            || trimmed.starts_with("BlockSz")
        {
            continue;
        }
        if trimmed.starts_with("Total") {
            let numbers = trimmed
                .split_once('=')
                .map(|(_, rest)| rest)
                .unwrap_or(trimmed)
                .split_whitespace()
                .filter_map(|token| token.replace(',', "").parse::<u64>().ok())
                .collect::<Vec<_>>();
            if numbers.len() >= 4 {
                total_loaders = numbers[0];
                total_classes = numbers[1];
                total_chunks = numbers[2];
                total_blocks = numbers[3];
            }
            continue;
        }
        let is_continuation = raw_line.starts_with(char::is_whitespace) && !trimmed.starts_with("0x");
        if is_continuation {
            if let (Some(index), Some(first)) = (previous, trimmed.split_whitespace().next()) {
                if trimmed.contains("hidden classes") {
                    if let Ok(count) = first.parse::<u64>() {
                        if let Some(loader) = loaders.get_mut(index) {
                            loader.hidden_classes = count;
                        }
                    }
                }
            }
            continue;
        }
        if !trimmed.starts_with("0x") {
            continue;
        }
        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 6 {
            continue;
        }
        let Ok(classes) = tokens[3].parse::<u64>() else {
            continue;
        };
        let chunk_size = parse_compact_size(tokens[4]);
        let block_size = parse_compact_size(tokens[5]);
        let type_start = if tokens[5]
            .chars()
            .next()
            .map(|ch| ch.is_ascii_digit())
            .unwrap_or(false)
        {
            6
        } else {
            5
        };
        let kind = tokens[type_start..].join(" ");
        let kind = if kind.contains("<boot class loader>") {
            "Bootstrap".to_string()
        } else if kind.contains("PlatformClassLoader") {
            "Platform".to_string()
        } else if kind.contains("AppClassLoader") {
            "Application".to_string()
        } else {
            "Custom".to_string()
        };
        loaders.push(ClassLoaderInfo {
            id: tokens[0].to_string(),
            parent_id: tokens[1].to_string(),
            cld: tokens[2].to_string(),
            classes,
            chunk_size,
            block_size,
            kind,
            hidden_classes: 0,
        });
        previous = Some(loaders.len() - 1);
    }

    if total_loaders == 0 {
        total_loaders = loaders.len() as u64;
    }
    if total_classes == 0 {
        total_classes = loaders.iter().map(|loader| loader.classes).sum();
    }
    ClassLoaderStats {
        loaders,
        total_loaders,
        total_classes,
        total_chunks,
        total_blocks,
    }
}

fn parse_compact_size(value: &str) -> u64 {
    let cleaned = value.replace(',', "");
    let split = cleaned
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(cleaned.len());
    let Ok(number) = cleaned[..split].parse::<f64>() else {
        return 0;
    };
    let multiplier = match cleaned[split..].trim().to_ascii_lowercase().as_str() {
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (number * multiplier).round() as u64
}

pub fn parse_class_histogram(text: &str) -> (Vec<ClassHistogramRow>, u64, u64) {
    let mut rows = Vec::new();
    let mut total_instances = 0;
    let mut total_bytes = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("num ")
            || trimmed.starts_with("---")
            || trimmed.starts_with("Total")
        {
            if trimmed.starts_with("Total") {
                let numbers = trimmed
                    .split_whitespace()
                    .filter_map(|token| token.replace(',', "").parse::<u64>().ok())
                    .collect::<Vec<_>>();
                if numbers.len() >= 2 {
                    total_instances = numbers[0];
                    total_bytes = numbers[1];
                }
            }
            continue;
        }
        let Some((rank_text, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let Ok(rank) = rank_text.trim().parse::<u32>() else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(instances) = parts.next().and_then(|value| value.replace(',', "").parse::<u64>().ok()) else {
            continue;
        };
        let Some(bytes) = parts.next().and_then(|value| value.replace(',', "").parse::<u64>().ok()) else {
            continue;
        };
        let class_and_module = parts.collect::<Vec<_>>().join(" ");
        let (class_name, module) = split_module(&class_and_module);
        rows.push(ClassHistogramRow {
            rank,
            instances,
            bytes,
            class_name,
            module,
        });
    }
    if total_instances == 0 {
        total_instances = rows.iter().map(|row| row.instances).sum();
        total_bytes = rows.iter().map(|row| row.bytes).sum();
    }
    (rows, total_instances, total_bytes)
}

fn split_module(value: &str) -> (String, Option<String>) {
    if let Some((name, module)) = value.rsplit_once(" (") {
        if module.ends_with(')') {
            return (
                name.to_string(),
                Some(module.trim_end_matches(')').to_string()),
            );
        }
    }
    (value.to_string(), None)
}

pub fn read_class_loading(jcmd: &Path, pid: u32) -> Result<ClassLoadingSnapshot> {
    let loaders_text = run_jcmd(jcmd, pid, "VM.classloader_stats", &[], 15_000)
        .map_err(|err| JccError::attach(format!("could not read class loader statistics: {err}")))?;
    let stats = parse_classloader_stats(&loaders_text);
    let mut notes = vec![
        "Class loader data comes from VM.classloader_stats; class counts include classes that are still loaded."
            .to_string(),
        "The histogram is a point-in-time snapshot and may briefly pause the target JVM."
            .to_string(),
    ];
    let (histogram, total_instances, total_bytes) =
        match run_jcmd(jcmd, pid, "GC.class_histogram", &[], 45_000) {
            Ok(text) => parse_class_histogram(&text),
            Err(err) => {
                notes.push(format!("Class histogram unavailable: {err}"));
                (Vec::new(), 0, 0)
            }
        };

    Ok(ClassLoadingSnapshot {
        pid,
        timestamp: now_ms(),
        loaders: stats.loaders,
        total_classes: stats.total_classes,
        total_chunks: stats.total_chunks,
        total_blocks: stats.total_blocks,
        histogram,
        total_instances,
        total_bytes,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADERS: &str = r#"ClassLoader         Parent              CLD*               Classes   ChunkSz   BlockSz  Type
0x0000000000000000  0x0000000000000000  0x000001621e2e9f90     695   4456448    190160  <boot class loader>
                                                                 1      3072      1600   + hidden classes
0x000001621f069fd8  0x000001621f06a860  0x000001621e682640       3      8192      6424  jdk.internal.loader.ClassLoaders$AppClassLoader
0x000001621f06a860  0x0000000000000000  0x000001621e2e9a10       0         0         0  jdk.internal.loader.ClassLoaders$PlatformClassLoader
Total = 3                                                      699   4467712    198184
ChunkSz: Total size of all allocated metaspace chunks
BlockSz: Total size of all allocated metaspace blocks (each chunk has several blocks)
"#;

    const HISTOGRAM: &str = r#" num     #instances         #bytes  class name (module)
-------------------------------------------------------
   1:          2922      104967448  [B (java.base@21.0.9)
   2:           708          88528  java.lang.Class (java.base@21.0.9)
   3:            11           37704  [C (java.base@21.0.9)
Total          3641      105093680
"#;

    #[test]
    fn parses_class_loader_stats() {
        let stats = parse_classloader_stats(LOADERS);
        assert_eq!(stats.loaders.len(), 3);
        assert_eq!(stats.total_classes, 699);
        assert_eq!(stats.loaders[0].kind, "Bootstrap");
        assert_eq!(stats.loaders[0].hidden_classes, 1);
        assert_eq!(stats.loaders[1].kind, "Application");
        assert_eq!(stats.loaders[2].kind, "Platform");
        assert_eq!(stats.loaders[0].chunk_size, 4_456_448);
    }

    #[test]
    fn parses_histogram_and_module() {
        let (rows, total_instances, total_bytes) = parse_class_histogram(HISTOGRAM);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].class_name, "[B");
        assert_eq!(rows[0].module.as_deref(), Some("java.base@21.0.9"));
        assert_eq!(rows[0].bytes, 104_967_448);
        assert_eq!(total_instances, 3641);
        assert_eq!(total_bytes, 105_093_680);
    }
}
