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

/// Contrôles qui doivent avoir réellement tourné pour qu'un `pass` puisse
/// être auto-approuvé. Un `not_applicable` (modèle absent, projet absent,
/// contrôle désactivé) interdit l'auto-approbation : le rapport reste
/// exploitable, mais une revue humaine est requise.
const REQUIRED_AUTO_APPROVE_CHECKS: &[&str] = &[
    "project_readable",
    "subtitle_ocr",
    "logo_residual",
    "face_cut",
];

/// Décide si le rapport peut être auto-approuvé et explique le refus.
///
/// L'auto-approbation n'est possible que sur un `pass` complet, avec les
/// contrôles requis exécutés. Le vérificateur visuel, quand il est présent,
/// doit lui aussi être `pass` ou `info` : un modèle DINOv2 absent ne doit pas
/// produire un `pass` silencieusement moins fiable.
pub fn auto_approve(checks: &[QaCheck], findings: &[QaFinding]) -> (bool, String) {
    let status = overall_status(checks, findings);
    if status != QaStatus::Pass {
        return (
            false,
            format!("statut {} : revue humaine requise", status.as_str()),
        );
    }

    let mut missing = Vec::new();
    for id in REQUIRED_AUTO_APPROVE_CHECKS {
        match checks.iter().find(|check| check.id == *id) {
            Some(check) if matches!(check.status, QaStatus::Pass | QaStatus::Info) => {}
            Some(check) => missing.push(format!("{id}={}", check.status.as_str())),
            None => missing.push(format!("{id}=absent")),
        }
    }
    if let Some(check) = checks.iter().find(|check| check.id == "logo_visual") {
        if !matches!(check.status, QaStatus::Pass | QaStatus::Info) {
            missing.push(format!("logo_visual={}", check.status.as_str()));
        }
    }

    if !missing.is_empty() {
        return (
            false,
            format!(
                "contrôles requis non exécutés ou non conformes: {}",
                missing.join(", ")
            ),
        );
    }

    (
        true,
        "tous les contrôles requis sont pass; aucun finding".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::{DefectType, Severity};

    fn finding(defect_type: DefectType) -> QaFinding {
        QaFinding::new(defect_type, 1.0, None, Severity::Low, 0.5, "test", "test")
    }

    fn required_checks() -> Vec<QaCheck> {
        vec![
            QaCheck::pass("project_readable", "1 segment"),
            QaCheck::pass("subtitle_ocr", "0 finding"),
            QaCheck::pass("logo_residual", "0 finding"),
            QaCheck::pass("face_cut", "0 finding"),
        ]
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

    #[test]
    fn pass_with_required_checks_is_auto_approvable() {
        let (eligible, reason) = auto_approve(&required_checks(), &[]);
        assert!(eligible);
        assert!(reason.contains("aucun finding"));
    }

    #[test]
    fn auto_approve_is_refused_on_review_or_fail() {
        let mut checks = required_checks();
        checks.push(QaCheck::warn("audio_codec", "opus"));
        let (eligible, reason) = auto_approve(&checks, &[]);
        assert!(!eligible);
        assert!(reason.contains("review"));

        let findings = vec![finding(DefectType::SpecMismatch)];
        let (eligible, reason) = auto_approve(&required_checks(), &findings);
        assert!(!eligible);
        assert!(reason.contains("fail"));
    }

    #[test]
    fn auto_approve_requires_every_check_to_have_run() {
        for id in REQUIRED_AUTO_APPROVE_CHECKS {
            let checks = required_checks()
                .into_iter()
                .map(|check| {
                    if check.id == *id {
                        QaCheck::not_applicable(*id, "absent")
                    } else {
                        check
                    }
                })
                .collect::<Vec<_>>();
            let (eligible, reason) = auto_approve(&checks, &[]);
            assert!(!eligible, "{id} devrait bloquer");
            assert!(reason.contains(id));
        }
    }

    #[test]
    fn missing_required_check_blocks_auto_approve() {
        let checks = required_checks()
            .into_iter()
            .filter(|check| check.id != "face_cut")
            .collect::<Vec<_>>();
        let (eligible, reason) = auto_approve(&checks, &[]);
        assert!(!eligible);
        assert!(reason.contains("face_cut=absent"));
    }

    #[test]
    fn visual_check_must_run_when_present() {
        let mut checks = required_checks();
        checks.push(QaCheck::not_applicable("logo_visual", "modèle absent"));
        let (eligible, reason) = auto_approve(&checks, &[]);
        assert!(!eligible);
        assert!(reason.contains("logo_visual=not_applicable"));
    }

    #[test]
    fn absent_visual_check_does_not_block() {
        // `--no-logo-visual` ne pousse aucun check `logo_visual` : le
        // contrôle texte explicitement choisi reste auto-approbable.
        let (eligible, _) = auto_approve(&required_checks(), &[]);
        assert!(eligible);
    }
}
