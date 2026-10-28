//! Modèles sérialisables du post-traitement A5/A6.

use serde::Serialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Apply,
    Review,
    Noop,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Review => "review",
            Self::Noop => "noop",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub code: &'static str,
    pub action: Action,
    pub target: String,
    pub reason: String,
    pub confidence: f64,
    pub details: Value,
}

impl Decision {
    pub fn new(
        code: &'static str,
        action: Action,
        target: impl Into<String>,
        reason: impl Into<String>,
        confidence: f64,
        details: Value,
    ) -> Self {
        Self {
            code,
            action,
            target: target.into(),
            reason: reason.into(),
            confidence,
            details,
        }
    }

    pub fn apply(
        code: &'static str,
        target: impl Into<String>,
        reason: impl Into<String>,
        confidence: f64,
        details: Value,
    ) -> Self {
        Self::new(code, Action::Apply, target, reason, confidence, details)
    }

    pub fn review(
        code: &'static str,
        target: impl Into<String>,
        reason: impl Into<String>,
        confidence: f64,
        details: Value,
    ) -> Self {
        Self::new(code, Action::Review, target, reason, confidence, details)
    }

    pub fn noop(
        code: &'static str,
        target: impl Into<String>,
        reason: impl Into<String>,
        confidence: f64,
        details: Value,
    ) -> Self {
        Self::new(code, Action::Noop, target, reason, confidence, details)
    }

    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("code".into(), json!(self.code));
        object.insert("action".into(), json!(self.action.as_str()));
        object.insert("target".into(), json!(self.target));
        object.insert("reason".into(), json!(self.reason));
        object.insert("confidence".into(), json!(self.confidence));
        object.insert("details".into(), self.details.clone());
        Value::Object(object)
    }

    pub fn to_review_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("code".into(), json!(self.code));
        object.insert(
            "severity".into(),
            json!(if matches!(self.code, "A1" | "A2" | "A6") {
                "high"
            } else {
                "medium"
            }),
        );
        object.insert("target".into(), json!(self.target));
        object.insert("reason".into(), json!(self.reason));
        object.insert("confidence".into(), json!(self.confidence));
        object.insert("details".into(), self.details.clone());
        Value::Object(object)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PostSummary {
    pub apply: usize,
    pub review: usize,
    pub noop: usize,
    pub a3_candidates: usize,
    pub a6_eligible: usize,
    pub template_applied: bool,
}

impl PostSummary {
    pub fn from_decisions(decisions: &[Decision]) -> Self {
        Self {
            apply: decisions
                .iter()
                .filter(|decision| decision.action == Action::Apply)
                .count(),
            review: decisions
                .iter()
                .filter(|decision| decision.action == Action::Review)
                .count(),
            noop: decisions
                .iter()
                .filter(|decision| decision.action == Action::Noop)
                .count(),
            a3_candidates: decisions
                .iter()
                .filter(|decision| {
                    decision.code == "A3"
                        && decision.action == Action::Review
                        && decision.details.get("candidate_keys").is_some()
                })
                .count(),
            a6_eligible: decisions
                .iter()
                .filter(|decision| decision.code == "A6" && decision.action == Action::Apply)
                .count(),
            template_applied: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PostProjectOutcome {
    pub project: String,
    pub assist_project: Option<String>,
    pub artifacts_dir: Option<String>,
    pub summary: PostSummary,
    pub audit_verdict: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PostRunSummary {
    pub projects: usize,
    pub assists: usize,
    pub apply: usize,
    pub review: usize,
    pub noop: usize,
    pub failures: Vec<String>,
    pub items: Vec<PostProjectOutcome>,
}

impl PostRunSummary {
    pub fn push(&mut self, outcome: PostProjectOutcome) {
        self.projects += 1;
        if outcome.assist_project.is_some() {
            self.assists += 1;
        }
        self.apply += outcome.summary.apply;
        self.review += outcome.summary.review;
        self.noop += outcome.summary.noop;
        if outcome.error.is_some() {
            self.failures.push(outcome.project.clone());
        }
        self.items.push(outcome);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoHash {
    pub mode: &'static str,
    pub size: u64,
    pub chunk: u64,
    pub offsets: Vec<u64>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditCheck {
    pub id: String,
    pub status: &'static str,
    pub detail: String,
}

impl AuditCheck {
    pub fn pass(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "pass",
            detail: detail.into(),
        }
    }

    pub fn warn(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "warn",
            detail: detail.into(),
        }
    }

    pub fn fail(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "fail",
            detail: detail.into(),
        }
    }
}

pub fn verdict(checks: &[AuditCheck]) -> &'static str {
    if checks.iter().any(|check| check.status == "fail") {
        "fail"
    } else if checks.iter().any(|check| check.status == "warn") {
        "warn"
    } else {
        "pass"
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}
