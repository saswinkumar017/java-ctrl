use crate::core::error::Result;
use crate::core::models::{CodeHeapInfo, CompiledMethod, JitSnapshot};
use crate::core::monitor::{parse_code_cache, parse_perf_counters, ticks_to_ms};
use crate::core::tools::{now_ms, run_jcmd};
use std::path::Path;

pub fn parse_codelist(text: &str) -> Vec<CompiledMethod> {
    let mut methods = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (head, range) = match trimmed.split_once(" [") {
            Some((head, range)) => (head, format!("[{range}")),
            None => (trimmed, String::new()),
        };
        let tokens = head.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 4 {
            continue;
        }
        let Ok(id) = tokens[0].parse::<u64>() else {
            continue;
        };
        let Ok(level) = tokens[1].parse::<u32>() else {
            continue;
        };
        let Ok(sync) = tokens[2].parse::<u32>() else {
            continue;
        };
        methods.push(CompiledMethod {
            id,
            level,
            sync,
            method: tokens[3..].join(" "),
            address_range: range,
        });
    }
    methods
}

pub fn parse_compiler_queue(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub fn read_jit(jcmd: &Path, pid: u32) -> Result<JitSnapshot> {
    let mut notes = Vec::new();
    let counters_text = run_jcmd(jcmd, pid, "PerfCounter.print", &[], 10_000)?;
    let counters = parse_perf_counters(&counters_text);

    let code_cache = match run_jcmd(jcmd, pid, "Compiler.codecache", &[], 10_000)
        .and_then(|text| parse_code_cache(&text))
    {
        Ok(snapshot) => snapshot,
        Err(err) => {
            notes.push(format!("Code cache details unavailable: {err}"));
            Default::default()
        }
    };

    let queue = match run_jcmd(jcmd, pid, "Compiler.queue", &[], 10_000) {
        Ok(text) => parse_compiler_queue(&text),
        Err(err) => {
            notes.push(format!("Compiler queue unavailable: {err}"));
            Vec::new()
        }
    };

    let mut compiled_methods = match run_jcmd(jcmd, pid, "Compiler.codelist", &[], 20_000) {
        Ok(text) => parse_codelist(&text),
        Err(err) => {
            notes.push(format!("Compiled method list unavailable: {err}"));
            Vec::new()
        }
    };
    let total_compiled_methods = compiled_methods.len() as u64;
    if compiled_methods.len() > 1_500 {
        compiled_methods.truncate(1_500);
        notes.push(
            "The compiled method list is capped at 1,500 entries in the UI. Statistics cover all methods."
                .to_string(),
        );
    }

    let number = |key: &str| {
        counters
            .get(key)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };

    Ok(JitSnapshot {
        pid,
        timestamp: now_ms(),
        compiles: number("sun.ci.totalCompiles"),
        time_ms: ticks_to_ms(&counters, number("java.ci.totalTime")),
        bailouts: number("sun.ci.totalBailouts"),
        invalidates: number("sun.ci.totalInvalidates"),
        nmethods: code_cache.nmethods,
        nmethod_code_size: number("sun.ci.nmethodCodeSize"),
        code_heaps: code_cache.heaps.clone(),
        code_total: code_cache.total_size,
        code_used: code_cache.used,
        code_max_used: code_cache.max_used,
        code_free: code_cache.free,
        compilation_enabled: code_cache.compilation_enabled,
        compiled_methods,
        total_compiled_methods,
        compiler_threads: number("sun.ci.threads") as u32,
        queue,
        notes,
    })
}

pub fn code_heaps_or_empty(heaps: &[CodeHeapInfo]) -> Vec<CodeHeapInfo> {
    heaps.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODELIST: &str = r#"24 0 0 java.lang.System.arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V [0x00000162130a0010, 0x00000162130a01c0 - 0x00000162130a0418]
23 1 0 java.lang.module.ModuleDescriptor.name()Ljava/lang/String; [0x00000162130a0490, 0x00000162130a0620 - 0x00000162130a06e8]
26 3 1 com.example.HotPath.compute(J)I [0x00000162130a0790, 0x00000162130a0920 - 0x00000162130a09e8]
"#;

    #[test]
    fn parses_compiled_methods() {
        let methods = parse_codelist(CODELIST);
        assert_eq!(methods.len(), 3);
        assert_eq!(methods[0].id, 24);
        assert_eq!(methods[0].level, 0);
        assert_eq!(
            methods[0].method,
            "java.lang.System.arraycopy(Ljava/lang/Object;ILjava/lang/Object;II)V"
        );
        assert_eq!(methods[2].level, 3);
        assert_eq!(methods[2].sync, 1);
        assert!(methods[0].address_range.starts_with('['));
    }

    #[test]
    fn parses_queue() {
        let queue = parse_compiler_queue("Current compiles: \n\nC1 compile queue:\nEmpty\n\nC2 compile queue:\nEmpty\n");
        assert_eq!(queue, vec!["Current compiles:", "C1 compile queue:", "Empty", "C2 compile queue:", "Empty"]);
    }
}
