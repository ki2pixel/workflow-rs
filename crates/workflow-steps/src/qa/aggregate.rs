//! Stage 2 — agrégation conservatrice des contrôles et findings.
//!
//! Règle : un défaut objectif (`spec_mismatch`, `template_logo_missing`) ou un
//! check `fail` bloque ; tout finding restant ou check `review` demande une
//! revue ; `info` et `not_applicable` ne dégradent pas le statut.

use crate::qa::model::{QaCheck, QaFinding, QaStatus};

pub fn overall_status(checks: &[QaCheck], findings: &[QaFinding]) -> QaStatus {
    let blocking_check = checks.iter().any(|check| check.status == QaStatus::Fail);
    let blocking_finding = findings
        .iter()
        .any(|finding| finding.defect_type.is_blocking());
    if blocking_check || blocking_finding {
        return QaStatus::Fail;
    }
    let review_check = checks.iter().any(|check| check.status == QaStatus::Review);
    if review_check || !findings.is_empty() {
        return QaStatus::Review;
    }
    QaStatus::Pass
}

pub fn auto_approve_reason() -> &'static str {
    "v1 : validation humaine requise (logos candidats, labels à revoir)"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::{DefectType, Severity};

    fn finding(defect_type: DefectType) -> QaFinding {
        QaFinding::new(defect_type, 1.0, None, Severity::Low, 0.5, "test", "test")
    }

    #[test]
    fn info_and_not_applicable_do_not_degrade() {
        let checks = vec![
            QaCheck::info("bitrate", "hors plage"),
            QaCheck::not_applicable("template_a6", "hors template"),
        ];
        assert_eq!(overall_status(&checks, &[]), QaStatus::Pass);
    }

    #[test]
    fn warn_check_requests_review() {
        let checks = vec![QaCheck::warn("audio_codec", "opus au lieu de aac")];
        assert_eq!(overall_status(&checks, &[]), QaStatus::Review);
    }

    #[test]
    fn subtitle_finding_requests_review() {
        let findings = vec![finding(DefectType::SubtitleBurnedIn)];
        assert_eq!(overall_status(&[], &findings), QaStatus::Review);
    }

    #[test]
    fn spec_mismatch_is_blocking() {
        let findings = vec![finding(DefectType::SpecMismatch)];
        assert_eq!(overall_status(&[], &findings), QaStatus::Fail);
    }

    #[test]
    fn fail_beats_review() {
        let checks = vec![QaCheck::warn("audio_codec", "opus")];
        let findings = vec![finding(DefectType::TemplateLogoMissing)];
        assert_eq!(overall_status(&checks, &findings), QaStatus::Fail);
    }
}
