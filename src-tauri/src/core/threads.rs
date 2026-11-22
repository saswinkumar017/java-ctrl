use crate::core::models::{ThreadInfo, ThreadSnapshot};
use crate::core::tools::now_ms;
use std::collections::BTreeMap;

pub fn parse_thread_dump(text: &str, pid: u32) -> ThreadSnapshot {
    let mut snapshot = ThreadSnapshot {
        pid,
        timestamp: now_ms(),
        raw: text.to_string(),
        ..Default::default()
    };
    let mut current: Option<ThreadInfo> = None;
    let mut in_locks = false;
    let mut deadlock_start: Option<usize> = None;
    let lines = text.lines().collect::<Vec<_>>();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if snapshot.generated_at.is_none()
            && !trimmed.is_empty()
            && !trimmed.starts_with("Full thread dump")
            && !trimmed.starts_with("Threads class")
        {
            snapshot.generated_at = Some(trimmed.to_string());
        }
        if is_deadlock_header(trimmed) {
            if let Some(thread) = current.take() {
                snapshot.threads.push(thread);
            }
            deadlock_start = Some(index);
            break;
        }
        if trimmed.starts_with('"') {
            if let Some(thread) = current.take() {
                snapshot.threads.push(thread);
            }
            current = Some(parse_thread_header(trimmed));
            in_locks = false;
            continue;
        }
        let Some(thread) = current.as_mut() else {
            continue;
        };
        if trimmed.starts_with("Locked ownable synchronizers:")
            || trimmed.starts_with("Locked synchronizers:")
        {
            in_locks = true;
            thread.locks.push(trimmed.to_string());
            continue;
        }
        if trimmed.starts_with("java.lang.Thread.State:") {
            let state = trimmed
                .trim_start_matches("java.lang.Thread.State:")
                .trim()
                .to_string();
            if let Some(open) = state.find('(') {
                thread.state = state[..open].trim().to_string();
                thread.state_detail = Some(
                    state[open..]
                        .trim()
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                        .to_string(),
                );
            } else {
                thread.state = state;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if in_locks {
            thread.locks.push(trimmed.to_string());
        } else {
            thread.stack.push(trimmed.to_string());
            if trimmed.contains("locked <")
                || trimmed.contains("waiting to lock")
                || trimmed.contains("parking to wait for")
                || trimmed.contains("waiting on")
            {
                thread.locks.push(trimmed.to_string());
            }
        }
    }
    if let Some(thread) = current.take() {
        snapshot.threads.push(thread);
    }

    if let Some(start) = deadlock_start {
        snapshot.deadlock = true;
        snapshot.deadlock_text = Some(lines[start..].join("\n"));
    }

    for thread in &snapshot.threads {
        let state = if thread.state.is_empty() {
            "UNKNOWN".to_string()
        } else {
            thread.state.clone()
        };
        *snapshot.counts.entry(state).or_insert(0) += 1;
    }
    snapshot.total = snapshot.threads.len() as u32;
    snapshot
}

fn is_deadlock_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.starts_with("found ") && lower.contains("deadlock"))
        || lower.starts_with("java-level deadlock")
}

fn parse_thread_header(line: &str) -> ThreadInfo {
    let mut info = ThreadInfo {
        header: line.to_string(),
        ..Default::default()
    };
    let after_quote = &line[1..];
    let Some(end_quote) = after_quote.find('"') else {
        return info;
    };
    info.name = after_quote[..end_quote].to_string();
    let rest = after_quote[end_quote + 1..].trim();
    let tokens = rest.split_whitespace().collect::<Vec<_>>();
    let mut state_tokens = Vec::new();
    let mut seen_nid = false;
    for token in &tokens {
        if let Some(value) = token.strip_prefix('#') {
            info.number = value.parse().ok();
            continue;
        }
        if token.starts_with('[') && token.ends_with(']') && info.number.is_some() && info.nid.is_none() {
            // Some JDKs print the OS thread id in brackets after the thread number.
            continue;
        }
        if *token == "daemon" {
            info.daemon = true;
            continue;
        }
        if let Some(value) = token.strip_prefix("prio=") {
            info.priority = value.parse().ok();
            continue;
        }
        if let Some(value) = token.strip_prefix("os_prio=") {
            info.os_priority = value.parse().ok();
            continue;
        }
        if let Some(value) = token.strip_prefix("cpu=") {
            info.cpu_ms = parse_duration_ms(value);
            continue;
        }
        if let Some(value) = token.strip_prefix("elapsed=") {
            info.elapsed_secs = parse_duration_secs(value);
            continue;
        }
        if let Some(value) = token.strip_prefix("tid=") {
            info.tid = Some(value.to_string());
            continue;
        }
        if let Some(value) = token.strip_prefix("nid=") {
            info.nid = Some(value.to_string());
            seen_nid = true;
            continue;
        }
        if token.starts_with('[') {
            // Stack pointer, not part of the state description.
            continue;
        }
        if seen_nid {
            state_tokens.push(*token);
        }
    }
    if !state_tokens.is_empty() {
        info.state = state_tokens.join(" ");
    }
    info
}

fn parse_duration_ms(value: &str) -> f64 {
    if let Some(value) = value.strip_suffix("ms") {
        value.parse().unwrap_or(0.0)
    } else if let Some(value) = value.strip_suffix('s') {
        value.parse::<f64>().unwrap_or(0.0) * 1000.0
    } else if let Some(value) = value.strip_suffix("us") {
        value.parse::<f64>().unwrap_or(0.0) / 1000.0
    } else {
        value.parse().unwrap_or(0.0)
    }
}

fn parse_duration_secs(value: &str) -> f64 {
    if let Some(value) = value.strip_suffix('s') {
        value.parse().unwrap_or(0.0)
    } else if let Some(value) = value.strip_suffix("ms") {
        value.parse::<f64>().unwrap_or(0.0) / 1000.0
    } else {
        value.parse().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = r#"2026-11-22 21:35:26
Full thread dump Java HotSpot(TM) 64-Bit Server VM (21.0.9+7-LTS-338 mixed mode, sharing):

"main" #1 [13036] prio=5 os_prio=0 cpu=156.25ms elapsed=36.65s tid=0x0000016201652490 nid=13036 waiting on condition  [0x00000037b9dff000]
   java.lang.Thread.State: TIMED_WAITING (sleeping)
	at java.lang.Thread.sleep0(java.base@21.0.9/Native Method)
	at Probe.main(Probe.java:10)

"worker-1" #9 daemon prio=10 os_prio=2 cpu=0.00ms elapsed=36.61s tid=0x000001621e4472e0 nid=328 runnable
   java.lang.Thread.State: RUNNABLE
	at java.util.concurrent.locks.LockSupport.park(LockSupport.java:221)
	- parking to wait for  <0x00000000e1243b50> (a java.util.concurrent.locks.AbstractQueuedSynchronizer$ConditionObject)

   Locked ownable synchronizers:
	- None

"blocked-worker" #10 prio=5 os_prio=0 cpu=1.50s elapsed=12.00s tid=0x123 nid=456 in Object.wait() [0x0000000000000000]
   java.lang.Thread.State: BLOCKED (on object monitor)
	at com.example.Service.run(Service.java:42)
	- waiting to lock <0x00000000e1243b50> (a java.lang.Object)

Found one Java-level deadlock:
=============================
"blocked-worker":
  waiting to lock monitor 0x000001 (object 0x000002, a java.lang.Object),
  which is held by "worker-1"

Java stack information for the threads listed above:
===================================================
"worker-1":
	at com.example.Service.run(Service.java:42)
"#;

    #[test]
    fn parses_threads_and_deadlock() {
        let snapshot = parse_thread_dump(DUMP, 42);
        assert_eq!(snapshot.total, 3);
        assert_eq!(snapshot.counts.get("TIMED_WAITING"), Some(&1));
        assert_eq!(snapshot.counts.get("RUNNABLE"), Some(&1));
        assert_eq!(snapshot.counts.get("BLOCKED"), Some(&1));
        assert!(snapshot.deadlock);
        assert!(snapshot.deadlock_text.as_deref().unwrap().contains("Java-level deadlock"));
        let main = snapshot.threads.iter().find(|thread| thread.name == "main").unwrap();
        assert_eq!(main.number, Some(1));
        assert_eq!(main.nid.as_deref(), Some("13036"));
        assert_eq!(main.priority, Some(5));
        assert!((main.cpu_ms - 156.25).abs() < 0.001);
        assert!((main.elapsed_secs - 36.65).abs() < 0.001);
        assert_eq!(main.state, "TIMED_WAITING");
        assert_eq!(main.state_detail.as_deref(), Some("sleeping"));
        assert!(main.stack.iter().any(|line| line.contains("Probe.main")));
        let blocked = snapshot
            .threads
            .iter()
            .find(|thread| thread.name == "blocked-worker")
            .unwrap();
        assert_eq!(blocked.state, "BLOCKED");
        assert!(blocked.locks.iter().any(|line| line.contains("waiting to lock")));
        assert_eq!(blocked.cpu_ms, 1500.0);
    }
}
