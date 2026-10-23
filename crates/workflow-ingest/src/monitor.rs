//! Décision pure « quels liens télécharger lors de ce passage ».
//!
//! Le moniteur Python applique quatre filtres : schéma, doublon d'historique,
//! téléchargement déjà suivi, et cooldown exponentiel des échecs. Cette
//! fonction les reproduit sans effectuer d'I/O, ce qui la rend testable.

use crate::history::{cooldown_remaining_s, FailureRecord};
use crate::url::{
    classify_url_type, is_dropbox_proxy_url, is_dropbox_url, looks_like_archive_download,
    normalize_url, url_scheme,
};
use crate::webhook::WebhookRecord;
use chrono::NaiveDateTime;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadPlan {
    pub url: String,
    pub fallback_url: String,
    pub original_filename: String,
    pub provider: String,
    pub timestamp: String,
    pub url_type: String,
}

pub struct PlanContext<'a> {
    pub history: &'a HashSet<String>,
    pub failures: &'a HashMap<String, FailureRecord>,
    pub tracked: &'a HashSet<String>,
    pub now: NaiveDateTime,
    pub interval_s: u64,
    pub cooldown_max_s: u64,
}

pub fn plan_downloads(records: &[WebhookRecord], context: &PlanContext<'_>) -> Vec<DownloadPlan> {
    let mut plans = Vec::new();
    let mut handled: HashSet<String> = HashSet::new();

    for row in records {
        let url = row.url.trim();
        if url.is_empty() {
            continue;
        }
        let normalized = normalize_url(url);
        if normalized.is_empty() {
            continue;
        }
        let normalized_fallback = if row.fallback_url.trim().is_empty() {
            String::new()
        } else {
            normalize_url(row.fallback_url.trim())
        };
        if handled.contains(&normalized)
            || (!normalized_fallback.is_empty() && handled.contains(&normalized_fallback))
        {
            continue;
        }

        let in_history = context.history.contains(&normalized)
            || (!normalized_fallback.is_empty() && context.history.contains(&normalized_fallback));
        let is_tracked = context.tracked.contains(&normalized)
            || (!normalized_fallback.is_empty() && context.tracked.contains(&normalized_fallback));
        if is_tracked || in_history {
            handled.insert(normalized);
            if !normalized_fallback.is_empty() {
                handled.insert(normalized_fallback);
            }
            continue;
        }

        let failure = context
            .failures
            .get(&normalized)
            .or_else(|| context.failures.get(&normalized_fallback));
        if let Some(failure) = failure {
            let remaining = cooldown_remaining_s(
                failure,
                context.now,
                context.interval_s,
                context.cooldown_max_s,
            );
            if remaining > 0.0 {
                tracing::debug!(
                    "moniteur: cooldown {:.0}s pour {url} (kind={}, attempts={})",
                    remaining,
                    failure.kind,
                    failure.attempts
                );
                handled.insert(normalized);
                if !normalized_fallback.is_empty() {
                    handled.insert(normalized_fallback);
                }
                continue;
            }
            tracing::info!(
                "moniteur: nouvelle tentative pour {url} (kind={}, attempts={})",
                failure.kind,
                failure.attempts
            );
        }

        let scheme = url_scheme(url);
        if !scheme.is_empty() && scheme != "http" && scheme != "https" {
            handled.insert(normalized);
            if !normalized_fallback.is_empty() {
                handled.insert(normalized_fallback);
            }
            continue;
        }

        let provider = row.provider.trim().to_ascii_lowercase();
        let declared_type = row.url_type.trim().to_ascii_lowercase();
        let url_type = if !declared_type.is_empty() {
            declared_type
        } else if url.to_ascii_lowercase().contains("fromsmash.com") {
            "fromsmash".into()
        } else if url.to_ascii_lowercase().contains("swisstransfer.com") {
            "swisstransfer".into()
        } else if is_dropbox_url(url) || is_dropbox_proxy_url(url) || provider == "dropbox" {
            "dropbox".into()
        } else {
            classify_url_type(url).to_string()
        };

        let is_dropbox_like = url_type == "dropbox"
            || provider == "dropbox"
            || is_dropbox_url(url)
            || is_dropbox_proxy_url(url);
        let has_new_schema_hints = !row.original_filename.trim().is_empty()
            || !row.fallback_url.trim().is_empty()
            || is_dropbox_proxy_url(url);
        let auto_download_allowed = is_dropbox_like
            && looks_like_archive_download(url, &row.original_filename)
            && has_new_schema_hints;

        if auto_download_allowed {
            plans.push(DownloadPlan {
                url: url.to_string(),
                fallback_url: row.fallback_url.trim().to_string(),
                original_filename: row.original_filename.trim().to_string(),
                provider: row.provider.trim().to_string(),
                timestamp: row.timestamp.clone(),
                url_type,
            });
            handled.insert(normalized);
            if !normalized_fallback.is_empty() {
                handled.insert(normalized_fallback);
            }
        }
    }

    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(url: &str, fallback: &str, filename: &str) -> WebhookRecord {
        WebhookRecord {
            url: url.into(),
            fallback_url: fallback.into(),
            original_filename: filename.into(),
            provider: "dropbox".into(),
            timestamp: "2026-09-21 16:47:14".into(),
            source: "webhook".into(),
            url_type: "dropbox".into(),
        }
    }

    fn context<'a>(
        history: &'a HashSet<String>,
        failures: &'a HashMap<String, FailureRecord>,
        tracked: &'a HashSet<String>,
        now: NaiveDateTime,
    ) -> PlanContext<'a> {
        PlanContext {
            history,
            failures,
            tracked,
            now,
            interval_s: 15,
            cooldown_max_s: 3600,
        }
    }

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2026-09-21T16:48:00", "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn plans_an_eligible_r2_record_with_its_fallback() {
        let records = [record(
            "https://server.kidpixel.workers.dev/dropbox/a/b/file",
            "https://www.dropbox.com/scl/fo/tokenABC/Folder?rlkey=KEY&dl=1",
            "197 Camille.zip",
        )];
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &HashMap::new(), &HashSet::new(), now()),
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].original_filename, "197 Camille.zip");
        assert!(plans[0].fallback_url.contains("dropbox.com"));
    }

    #[test]
    fn skips_urls_already_in_history_or_tracked() {
        let url = "https://server.kidpixel.workers.dev/dropbox/a/b/file";
        let normalized = normalize_url(url);
        let records = [record(url, "", "197 Camille.zip")];
        let mut history = HashSet::new();
        history.insert(normalized.clone());
        let plans = plan_downloads(
            &records,
            &context(&history, &HashMap::new(), &HashSet::new(), now()),
        );
        assert!(plans.is_empty());

        let mut tracked = HashSet::new();
        tracked.insert(normalized);
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &HashMap::new(), &tracked, now()),
        );
        assert!(plans.is_empty());
    }

    #[test]
    fn respects_the_failure_cooldown_then_retries() {
        let url = "https://server.kidpixel.workers.dev/dropbox/a/b/file";
        let normalized = normalize_url(url);
        let mut failures = HashMap::new();
        failures.insert(
            normalized,
            FailureRecord {
                url: url.into(),
                kind: "not_found".into(),
                status_code: Some(404),
                attempts: 1,
                last_error: "missing".into(),
                first_seen_at: "2026-09-21T16:47:14".into(),
                last_attempt_at: "2026-09-21T16:47:50".into(),
            },
        );
        let records = [record(url, "", "197 Camille.zip")];
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &failures, &HashSet::new(), now()),
        );
        assert!(plans.is_empty());

        let later =
            NaiveDateTime::parse_from_str("2026-09-21T17:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &failures, &HashSet::new(), later),
        );
        assert_eq!(plans.len(), 1);
    }

    #[test]
    fn ignores_non_archive_and_external_links() {
        let mut preview = record(
            "https://www.dropbox.com/s/abc/preview.png",
            "",
            "preview.png",
        );
        preview.provider.clear();
        preview.url_type.clear();
        let mut external = record("https://fromsmash.com/file", "", "archive.zip");
        external.provider.clear();
        external.url_type.clear();
        let records = [preview, external];
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &HashMap::new(), &HashSet::new(), now()),
        );
        assert!(plans.is_empty());
    }

    #[test]
    fn deduplicates_the_same_normalized_url_in_one_pass() {
        let base = "https://www.dropbox.com/scl/fo/test123/TestFolder";
        let records = [
            record(
                &format!("{base}?amp%3Bdl=0&dl=1&rlkey=testkey"),
                "",
                "Test.zip",
            ),
            record(&format!("{base}?dl=1&rlkey=testkey"), "", "Test.zip"),
        ];
        let plans = plan_downloads(
            &records,
            &context(&HashSet::new(), &HashMap::new(), &HashSet::new(), now()),
        );
        assert_eq!(plans.len(), 1);
    }
}
