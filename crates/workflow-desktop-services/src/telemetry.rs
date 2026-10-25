//! Télémétrie locale minimale : RSS/PSS du processus GUI en JSONL.

use crate::error::ServicesError;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProcessMetrics {
    pub rss_kib: u64,
    pub pss_kib: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelemetrySample {
    pub timestamp: String,
    pub rss_kib: u64,
    pub pss_kib: Option<u64>,
}

impl TelemetrySample {
    pub fn now(metrics: ProcessMetrics) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            rss_kib: metrics.rss_kib,
            pss_kib: metrics.pss_kib,
        }
    }
}

pub fn read_self_metrics() -> Result<ProcessMetrics, ServicesError> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let rss_kib = parse_status_kib(&status, "VmRSS:").unwrap_or(0);
    let pss_kib = std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .and_then(|rollup| parse_status_kib(&rollup, "Pss:"));
    Ok(ProcessMetrics { rss_kib, pss_kib })
}

fn parse_status_kib(content: &str, key: &str) -> Option<u64> {
    content.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

pub struct TelemetryWriter {
    path: PathBuf,
}

impl TelemetryWriter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, sample: &TelemetrySample) -> Result<(), ServicesError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, sample)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_status_values() {
        let status = "Name:\tworkflow\nVmRSS:\t  12345 kB\n";
        assert_eq!(parse_status_kib(status, "VmRSS:"), Some(12345));
        assert_eq!(parse_status_kib(status, "Pss:"), None);
    }

    #[test]
    fn reads_own_metrics() {
        let metrics = read_self_metrics().unwrap();
        assert!(metrics.rss_kib > 0);
    }
}
