use crate::core::error::{JccError, Result};
use crate::core::models::{
    JvmProcess, SpringCandidate, SpringMeasurement, SpringMetric, SpringSnapshot,
};
use crate::core::tools::now_ms;
use base64::Engine;
use serde_json::Value;
use std::time::{Duration, Instant};
use ureq::Agent;

const REQUEST_TIMEOUT_MS: u64 = 4_000;
const DISCOVERY_TIMEOUT_MS: u64 = 1_800;

/// Metrics that answer the questions a JVM operator actually asks first.
/// Fetching every Actuator metric is both slow and noisy, so the probe focuses
/// on these and reports how many others are available.
const KEY_METRICS: &[&str] = &[
    "jvm.memory.used",
    "jvm.memory.max",
    "jvm.gc.pause",
    "jvm.threads.live",
    "jvm.classes.loaded",
    "process.cpu.usage",
    "process.uptime",
    "system.cpu.usage",
    "http.server.requests",
    "jvm.buffer.memory.used",
];

pub fn normalize_base_url(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(JccError::invalid("Actuator base URL cannot be empty"));
    }
    let mut base = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    while base.ends_with('/') {
        base.pop();
    }
    if base.ends_with("/actuator") {
        base.truncate(base.len() - "/actuator".len());
    }
    if base.is_empty() || !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(JccError::invalid(format!(
            "{input} is not a valid HTTP or HTTPS URL"
        )));
    }
    Ok(base)
}

pub fn probe_spring(
    base_url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<SpringSnapshot> {
    let base = normalize_base_url(base_url)?;
    let auth = auth_header(username, password);
    let agent = build_agent(REQUEST_TIMEOUT_MS);
    let mut notes = Vec::new();
    let checked_at = now_ms();

    let health_started = Instant::now();
    let health = match get_json(&agent, &format!("{base}/actuator/health"), auth.as_deref()) {
        Ok(value) => value,
        Err(err) => {
            notes.push(format!("Health endpoint unavailable: {err}"));
            Value::Null
        }
    };
    let latency_ms = health_started.elapsed().as_millis() as u64;

    let info = match get_json(&agent, &format!("{base}/actuator/info"), auth.as_deref()) {
        Ok(value) => value,
        Err(err) => {
            notes.push(format!("Info endpoint unavailable: {err}"));
            Value::Null
        }
    };

    let links = match get_json(&agent, &base, auth.as_deref()) {
        Ok(value) => value,
        Err(err) => {
            notes.push(format!("Actuator index unavailable: {err}"));
            Value::Null
        }
    };

    let metrics = match fetch_metrics(&agent, &base, auth.as_deref(), &mut notes) {
        Ok(metrics) => metrics,
        Err(err) => {
            notes.push(format!("Metrics endpoint unavailable: {err}"));
            Vec::new()
        }
    };

    if health.is_null() && info.is_null() && links.is_null() && metrics.is_empty() {
        return Err(JccError::http(format!(
            "No Spring Boot Actuator endpoint responded at {base}. Check that the application is running, Actuator is on the classpath, and the management endpoints are exposed."
        )));
    }

    Ok(SpringSnapshot {
        base_url: base,
        checked_at,
        health,
        info,
        links,
        metrics,
        latency_ms,
        notes,
    })
}

/// Look for Actuator endpoints on the ports advertised by Java processes.
/// A 401 or 403 still means an Actuator is there, so those count as reachable.
pub fn discover_candidates(processes: &[JvmProcess]) -> Vec<SpringCandidate> {
    let agent = build_agent(DISCOVERY_TIMEOUT_MS);
    let mut candidates = Vec::new();
    let mut seen: Vec<(u32, u16)> = Vec::new();

    for process in processes {
        for &port in &process.spring_ports {
            if seen.contains(&(process.pid, port)) {
                continue;
            }
            seen.push((process.pid, port));
            let url = format!("http://127.0.0.1:{port}");
            let mut reachable = false;
            let mut detail = None;
            for path in ["/actuator/health", "/actuator"] {
                match http_get(&agent, &format!("{url}{path}"), None) {
                    Ok(response) => {
                        reachable = true;
                        detail = Some(format!("{path} returned HTTP {}", response.status));
                        if response.status < 500 {
                            break;
                        }
                    }
                    Err(err) => detail = Some(err),
                }
            }
            candidates.push(SpringCandidate {
                pid: process.pid,
                port,
                url,
                detected_from: "process command line".to_string(),
                name: process.display_name.clone(),
                reachable,
                detail,
            });
        }
    }
    candidates
}

fn fetch_metrics(
    agent: &Agent,
    base: &str,
    auth: Option<&str>,
    notes: &mut Vec<String>,
) -> std::result::Result<Vec<SpringMetric>, String> {
    let listing = get_json(agent, &format!("{base}/actuator/metrics"), auth)?;
    let names = listing
        .get("names")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return Err("the metrics endpoint returned no metric names".to_string());
    }
    let mut selected = KEY_METRICS
        .iter()
        .filter(|name| names.iter().any(|available| available == *name))
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    if selected.is_empty() {
        selected = names.iter().take(8).cloned().collect();
    }
    notes.push(format!(
        "{} Actuator metrics are available; {} are shown.",
        names.len(),
        selected.len()
    ));

    let mut metrics = Vec::new();
    for name in selected {
        let encoded = percent_encoding::utf8_percent_encode(&name, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
        match get_json(agent, &format!("{base}/actuator/metrics/{encoded}"), auth) {
            Ok(value) => metrics.push(parse_metric(&name, &value)),
            Err(err) => notes.push(format!("Metric {name} unavailable: {err}")),
        }
    }
    Ok(metrics)
}

fn parse_metric(name: &str, value: &Value) -> SpringMetric {
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let base_unit = value
        .get("baseUnit")
        .and_then(Value::as_str)
        .map(str::to_string);
    let measurements = value
        .get("measurements")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(SpringMeasurement {
                        statistic: item.get("statistic")?.as_str()?.to_string(),
                        value: item.get("value")?.as_f64()?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let available_tags = value
        .get("availableTags")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("tag").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    SpringMetric {
        name: name.to_string(),
        description,
        base_unit,
        measurements,
        available_tags,
    }
}

struct HttpResponse {
    status: u16,
    text: String,
}

fn http_get(
    agent: &Agent,
    url: &str,
    auth: Option<&str>,
) -> std::result::Result<HttpResponse, String> {
    let mut request = agent.get(url);
    if let Some(auth) = auth {
        request = request.set("Authorization", auth);
    }
    match request.call() {
        Ok(response) => {
            let status = response.status();
            let text = response
                .into_string()
                .map_err(|err| format!("could not read response: {err}"))?;
            Ok(HttpResponse { status, text })
        }
        Err(ureq::Error::Status(status, response)) => {
            let text = response.into_string().unwrap_or_default();
            Ok(HttpResponse { status, text })
        }
        Err(err) => Err(err.to_string()),
    }
}

fn get_json(agent: &Agent, url: &str, auth: Option<&str>) -> std::result::Result<Value, String> {
    let response = http_get(agent, url, auth)?;
    if !(200..300).contains(&response.status) {
        let detail = truncate(response.text.trim(), 200);
        return Err(if detail.is_empty() {
            format!("HTTP {}", response.status)
        } else {
            format!("HTTP {}: {detail}", response.status)
        });
    }
    serde_json::from_str(&response.text).map_err(|err| format!("invalid JSON: {err}"))
}

fn build_agent(timeout_ms: u64) -> Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(timeout_ms))
        .timeout_read(Duration::from_millis(timeout_ms))
        .user_agent("JavaControlCentre/0.1")
        .build()
}

fn auth_header(username: Option<&str>, password: Option<&str>) -> Option<String> {
    let username = username?.trim();
    if username.is_empty() {
        return None;
    }
    let token = base64::engine::general_purpose::STANDARD
        .encode(format!("{username}:{}", password.unwrap_or_default()));
    Some(format!("Basic {token}"))
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut truncated = text.chars().take(limit).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_actuator_urls() {
        assert_eq!(
            normalize_base_url("localhost:8080/actuator/").expect("normalize"),
            "http://localhost:8080"
        );
        assert_eq!(
            normalize_base_url("https://example.test/management").expect("normalize"),
            "https://example.test/management"
        );
        assert!(normalize_base_url("   ").is_err());
    }

    #[test]
    fn parses_metric_payload() {
        let value: Value = serde_json::from_str(
            r#"{
                "name": "jvm.memory.used",
                "description": "The amount of used memory",
                "baseUnit": "bytes",
                "measurements": [{"statistic": "VALUE", "value": 1048576.0}],
                "availableTags": [{"tag": "area", "values": ["heap", "nonheap"]}]
            }"#,
        )
        .expect("parse fixture");
        let metric = parse_metric("jvm.memory.used", &value);
        assert_eq!(metric.base_unit.as_deref(), Some("bytes"));
        assert_eq!(metric.measurements.len(), 1);
        assert_eq!(metric.available_tags, vec!["area"]);
    }

    #[test]
    fn builds_basic_auth_header() {
        assert_eq!(
            auth_header(Some("admin"), Some("secret")).as_deref(),
            Some("Basic YWRtaW46c2VjcmV0")
        );
        assert!(auth_header(Some("  "), Some("secret")).is_none());
        assert!(auth_header(None, Some("secret")).is_none());
    }
}
