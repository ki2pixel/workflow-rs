//! Empreinte de cache du contrôle QA.
//!
//! Un `qa.json` n'est réutilisé que si l'export, le projet, les modèles et les
//! options qui changent le résultat sont identiques. L'empreinte est un
//! SHA-256 d'une chaîne canonique : elle ne dépend ni du chemin de sortie ni de
//! l'horloge. Le hash complet de l'export est déjà calculé pour `qa.json` ;
//! les hashes projet/modèles sont relus à chaque décision de cache.

use crate::post::detect;
use crate::qa::model::{Stage0Thresholds, QA_SCHEMA_VERSION};
use crate::qa::{face, ocr, QaOptions};
use crate::vision::dino;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// À incrémenter dès qu'un changement de code modifie les résultats pour les
/// mêmes export, projet, modèles et options (préprocessing OCR, seuils…).
const CACHE_VERSION: u32 = 8;

/// Empreinte complète : hash de l'export puis empreinte des autres entrées.
pub fn fingerprint(video: &Path, project: Option<&Path>, options: &QaOptions) -> Result<String> {
    let export_sha256 = detect::sha256_file(video)
        .with_context(|| format!("hachage SHA-256 de {}", video.display()))?;
    fingerprint_from_export(&export_sha256, project, options)
}

/// Variante utilisée par `run_qa`, qui a déjà hashé l'export.
pub fn fingerprint_from_export(
    export_sha256: &str,
    project: Option<&Path>,
    options: &QaOptions,
) -> Result<String> {
    let project_hash = match project {
        Some(path) => detect::sha256_file(path)
            .with_context(|| format!("hachage SHA-256 de {}", path.display()))?,
        None => "none".to_string(),
    };
    let thresholds = Stage0Thresholds::default();
    let canonical = format!(
        "cache_version={CACHE_VERSION}\n\
         schema={QA_SCHEMA_VERSION}\n\
         export={export_sha256}\n\
         project={project_hash}\n\
         model.det={}\n\
         model.rec={}\n\
         model.face={}\n\
         model.dino={}\n\
         model.probe={}\n\
         per_segment={}\n\
         ocr={}\n\
         logo={}\n\
         logo_visual={}\n\
         face={}\n\
         source_logo={}\n\
         source_logo_catalog={}\n\
         evidence={}\n\
         template_prefix={}\n\
         black={}\n\
         freeze={}\n\
         coverage={}\n\
         short_info={}\n\
         short_finding={}\n",
        model_hash(options, ocr::OCR_DET_FILENAME)?,
        model_hash(options, ocr::OCR_REC_FILENAME)?,
        model_hash(options, face::FACE_MODEL_FILENAME)?,
        model_hash_if(
            options,
            options.logo_visual_enabled,
            dino::DINO_MODEL_FILENAME
        )?,
        model_hash_if(
            options,
            options.logo_visual_enabled,
            dino::DINO_PROBE_FILENAME
        )?,
        options.per_segment,
        u8::from(options.ocr_enabled),
        u8::from(options.logo_enabled),
        u8::from(options.logo_visual_enabled),
        u8::from(options.face_enabled),
        u8::from(options.source_logo_enabled),
        source_logo_catalog_hash(options)?,
        u8::from(options.evidence_enabled),
        options.template_prefix,
        thresholds.black_finding_s,
        thresholds.freeze_finding_s,
        thresholds.coverage_warn_s,
        thresholds.short_segment_info_s,
        thresholds.short_segment_finding_s,
    );
    Ok(sha256_text(&canonical))
}

fn model_hash(options: &QaOptions, filename: &str) -> Result<String> {
    let Some(directory) = options.models_dir.as_deref() else {
        return Ok("none".into());
    };
    let path = directory.join(filename);
    if !path.is_file() {
        return Ok("missing".into());
    }
    detect::sha256_file(&path).with_context(|| format!("hachage du modèle {}", path.display()))
}

fn model_hash_if(options: &QaOptions, enabled: bool, filename: &str) -> Result<String> {
    if enabled {
        model_hash(options, filename)
    } else {
        Ok("none".into())
    }
}

/// Le catalogue spatial change la visibilité du logo : son contenu doit
/// invalider le cache, même quand seul le chemin par défaut est utilisé.
fn source_logo_catalog_hash(options: &QaOptions) -> Result<String> {
    if !options.source_logo_enabled {
        return Ok("none".into());
    }
    let path = options.source_logo_catalog.clone().or_else(|| {
        options
            .models_dir
            .as_ref()
            .map(|directory| directory.join(crate::vision::source_logo::CATALOG_FILENAME))
    });
    let Some(path) = path else {
        return Ok("missing".into());
    };
    if !path.is_file() {
        return Ok("missing".into());
    }
    detect::sha256_file(&path)
        .with_context(|| format!("hachage du catalogue de logos {}", path.display()))
}

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn options() -> QaOptions {
        QaOptions::default()
    }

    #[test]
    fn fingerprint_changes_with_export_content() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("export.mp4");
        fs::write(&video, b"first").unwrap();
        let first = fingerprint(&video, None, &options()).unwrap();
        fs::write(&video, b"second").unwrap();
        let second = fingerprint(&video, None, &options()).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn fingerprint_changes_with_project_content() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("export.mp4");
        let project = dir.path().join("export.ecproj");
        fs::write(&video, b"video").unwrap();
        fs::write(&project, b"one").unwrap();
        let first = fingerprint(&video, Some(&project), &options()).unwrap();
        fs::write(&project, b"two").unwrap();
        let second = fingerprint(&video, Some(&project), &options()).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn fingerprint_changes_with_result_options() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("export.mp4");
        fs::write(&video, b"video").unwrap();
        let base = fingerprint(&video, None, &options()).unwrap();

        let mut changed = options();
        changed.per_segment = 3;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());

        let mut changed = options();
        changed.evidence_enabled = false;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());

        let mut changed = options();
        changed.face_enabled = false;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());

        let mut changed = options();
        changed.logo_enabled = false;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());

        let mut changed = options();
        changed.logo_visual_enabled = false;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());

        let mut changed = options();
        changed.source_logo_enabled = true;
        assert_ne!(base, fingerprint(&video, None, &changed).unwrap());
    }

    #[test]
    fn fingerprint_is_stable_for_the_same_inputs() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("export.mp4");
        fs::write(&video, b"video").unwrap();
        let first = fingerprint(&video, None, &options()).unwrap();
        let second = fingerprint(&video, None, &options()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }
}
