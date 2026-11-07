//! Frames de preuve WebP pour les findings.
//!
//! Une frame est extraite par finding, à mi-intervalle quand l'intervalle est
//! connu, sinon 250 ms après le début. `ffmpeg` encode directement en WebP
//! dans `<out>/evidence/` : aucune dépendance d'image n'est ajoutée et les
//! fichiers restent petits. Un échec d'extraction ne fait pas échouer le QA :
//! il est compté et le finding reste sans preuve.

use crate::qa::model::QaFinding;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EvidenceStats {
    pub frames: usize,
    pub errors: usize,
    pub timing_ms: u64,
}

/// Écrit une frame par finding et renseigne `evidence_frame` (chemin relatif
/// au répertoire `--out`).
pub(crate) fn write_evidence_frames(
    video: &Path,
    findings: &mut [QaFinding],
    out_dir: &Path,
    ffmpeg: &Path,
    duration_s: f64,
) -> EvidenceStats {
    let started = Instant::now();
    let mut stats = EvidenceStats::default();
    if findings.is_empty() {
        return stats;
    }
    let directory = out_dir.join("evidence");
    if let Err(error) = fs::create_dir_all(&directory) {
        tracing::warn!(
            directory = %directory.display(),
            error = %error,
            "création du répertoire de preuves en échec"
        );
        stats.errors = findings.len();
        stats.timing_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        return stats;
    }

    for (index, finding) in findings.iter_mut().enumerate() {
        let time = evidence_time(finding, duration_s);
        let name = format!("{index:03}_{}_{time:.2}.webp", finding.defect_type.as_str());
        let destination = directory.join(&name);
        let temp = directory.join(format!(".{name}.tmp.webp"));
        let success = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{time:.3}"),
                "-i",
            ])
            .arg(video)
            .args([
                "-frames:v",
                "1",
                "-an",
                "-c:v",
                "libwebp",
                "-quality",
                "80",
                "-y",
            ])
            .arg(&temp)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if success && temp.is_file() {
            match fs::rename(&temp, &destination) {
                Ok(()) => {
                    finding.evidence_frame = Some(format!("evidence/{name}"));
                    stats.frames += 1;
                }
                Err(error) => {
                    let _ = fs::remove_file(&temp);
                    stats.errors += 1;
                    tracing::warn!(
                        finding = finding.defect_type.as_str(),
                        error = %error,
                        "publication de la preuve WebP en échec"
                    );
                }
            }
        } else {
            let _ = fs::remove_file(&temp);
            stats.errors += 1;
            tracing::warn!(
                time,
                finding = finding.defect_type.as_str(),
                "extraction de la preuve WebP en échec"
            );
        }
    }
    stats.timing_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    stats
}

/// Instant de la preuve, borné dans la durée de l'export.
fn evidence_time(finding: &QaFinding, duration_s: f64) -> f64 {
    let end = finding
        .timecode_end_sec
        .unwrap_or(finding.timecode_start_sec);
    let time = if end > finding.timecode_start_sec {
        (finding.timecode_start_sec + end) / 2.0
    } else {
        finding.timecode_start_sec + 0.25
    };
    time.clamp(0.0, (duration_s - 0.04).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::{DefectType, Severity};

    fn finding(start: f64, end: Option<f64>) -> QaFinding {
        QaFinding::new(
            DefectType::SubtitleBurnedIn,
            start,
            end,
            Severity::Medium,
            0.9,
            "test",
            "test",
        )
    }

    #[test]
    fn evidence_time_uses_the_interval_middle() {
        assert!((evidence_time(&finding(10.0, Some(20.0)), 60.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn evidence_time_returns_start_plus_quarter_second_without_end() {
        assert!((evidence_time(&finding(10.0, None), 60.0) - 10.25).abs() < 1e-9);
    }

    #[test]
    fn evidence_time_is_clamped_inside_the_export() {
        assert!((evidence_time(&finding(59.9, None), 60.0) - 59.96).abs() < 1e-9);
        assert!((evidence_time(&finding(80.0, None), 60.0) - 59.96).abs() < 1e-9);
    }

    #[test]
    fn defect_names_are_stable_for_paths() {
        assert_eq!(DefectType::SubtitleBurnedIn.as_str(), "subtitle_burned_in");
        assert_eq!(DefectType::FixedOverlay.as_str(), "fixed_overlay");
        assert_eq!(DefectType::FaceCut.as_str(), "face_cut");
    }
}
