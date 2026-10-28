//! Détection et génération directe du template SDM.
//!
//! Le bypass direct est volontairement conservateur : seules les vidéos dont
//! le nom commence par le préfixe configuré, en MP4 H.264 1920x1080 à 25 fps,
//! sont éligibles. Les autres continuent dans le pipeline normal.

use crate::post::ecproj::TICKS_PER_SECOND;
use crate::step2::windows::{scenes_csv_text, write_scenes_csv};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use workflow_schema::round::round_to;

pub const SOURCE_WIDTH: u32 = 1920;
pub const SOURCE_HEIGHT: u32 = 1080;
pub const COMP_WIDTH: u32 = 1080;
pub const COMP_HEIGHT: u32 = 1920;
pub const FPS: f64 = 25.0;
const FPS_TOLERANCE: f64 = 0.1;

#[derive(Debug, Clone, Serialize)]
pub struct TemplateProbe {
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub fps: f64,
    pub duration_s: f64,
    pub frames: u64,
    pub has_audio: bool,
    pub codec: String,
}

impl TemplateProbe {
    pub fn duration_ticks(&self) -> i64 {
        round_to(self.duration_s * TICKS_PER_SECOND, 0) as i64
    }

    fn matches_profile(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
            && self.width == SOURCE_WIDTH
            && self.height == SOURCE_HEIGHT
            && (self.pixel_aspect - 1.0).abs() < 1e-6
            && (self.fps - FPS).abs() <= FPS_TOLERANCE
            && self.codec.eq_ignore_ascii_case("h264")
            && self.duration_s > 0.0
            && self.frames > 0
    }
}

/// Vrai si le nom de fichier relève du template direct.
///
/// La frontière évite qu'un préfixe comme `W9 - LPHDF` capture
/// `W9 - LPHDFX`. Un préfixe terminé par un séparateur (`M6 - SDM - `) n'a
/// pas besoin de cette garde.
pub fn matches_name(path: &Path, prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let prefix_ends_with_separator = prefix
        .chars()
        .last()
        .is_some_and(|character| !character.is_alphanumeric());
    prefix_ends_with_separator
        || rest.is_empty()
        || !rest
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric())
}

/// Famille de bypass d'une vidéo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BypassMode {
    /// Template SDM : brut `.ecproj` + assist A6 après STEP7.
    Template,
    /// Archive seule : aucun `.ecproj`, seul le CSV de scènes est conservé.
    ArchiveOnly,
}

/// Préfixes actifs du bypass direct, résolus vers leur mode.
#[derive(Debug, Clone, Default)]
pub struct Bypass {
    entries: Vec<(String, BypassMode)>,
}

impl Bypass {
    pub fn new(template_prefix: &str, archive_only_prefixes: &[String]) -> Self {
        let mut entries = Vec::new();
        if !template_prefix.is_empty() {
            entries.push((template_prefix.to_string(), BypassMode::Template));
        }
        for prefix in archive_only_prefixes {
            if !prefix.is_empty() {
                entries.push((prefix.clone(), BypassMode::ArchiveOnly));
            }
        }
        Self { entries }
    }

    pub fn from_config(post: &workflow_core::config::PostConfig) -> Self {
        Self::new(&post.template_prefix, &post.archive_only_prefixes)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry_for_name(&self, path: &Path) -> Option<(&str, BypassMode)> {
        self.entries
            .iter()
            .find(|(prefix, _)| matches_name(path, prefix))
            .map(|(prefix, mode)| (prefix.as_str(), *mode))
    }

    /// Mode associé au nom, sans sonde ffprobe.
    pub fn mode_for_name(&self, path: &Path) -> Option<BypassMode> {
        self.entry_for_name(path).map(|(_, mode)| mode)
    }

    /// Sonde la vidéo si son nom relève d'un préfixe actif et que le profil
    /// 1920x1080 25 fps est valide.
    pub fn probe(
        &self,
        ffprobe: &Path,
        video: &Path,
    ) -> Result<Option<(BypassMode, TemplateProbe)>> {
        let Some((prefix, mode)) = self.entry_for_name(video) else {
            return Ok(None);
        };
        Ok(probe_direct(ffprobe, video, prefix)?.map(|probe| (mode, probe)))
    }
}

/// Sonde ffprobe commune aux étapes et au post-traitement.
pub fn probe(ffprobe: &Path, video: &Path) -> Result<TemplateProbe> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,width,height,sample_aspect_ratio,avg_frame_rate,r_frame_rate,nb_frames,duration",
            "-of",
            "json",
        ])
        .arg(video)
        .output()
        .with_context(|| format!("ffprobe introuvable ou en échec pour {}", video.display()))?;
    if !output.status.success() {
        bail!(
            "ffprobe: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let payload: Value = serde_json::from_slice(&output.stdout).context("ffprobe JSON invalide")?;
    let streams = payload
        .get("streams")
        .and_then(Value::as_array)
        .context("ffprobe sans flux")?;
    let video_stream = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        .context("ffprobe sans flux vidéo")?;
    let width = video_stream
        .get("width")
        .and_then(Value::as_u64)
        .context("ffprobe sans largeur")? as u32;
    let height = video_stream
        .get("height")
        .and_then(Value::as_u64)
        .context("ffprobe sans hauteur")? as u32;
    let fps = video_stream
        .get("avg_frame_rate")
        .and_then(Value::as_str)
        .and_then(parse_fraction)
        .or_else(|| {
            video_stream
                .get("r_frame_rate")
                .and_then(Value::as_str)
                .and_then(parse_fraction)
        })
        .context("ffprobe sans cadence vidéo")?;
    let duration_s = video_stream
        .get("duration")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0)
        .context("ffprobe sans durée de flux vidéo")?;
    let frames = video_stream
        .get("nb_frames")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| round_to(duration_s * FPS, 0).max(1.0) as u64);
    let pixel_aspect = match video_stream
        .get("sample_aspect_ratio")
        .and_then(Value::as_str)
    {
        Some(value) if !value.is_empty() && value != "0:1" => parse_fraction(value).unwrap_or(1.0),
        _ => 1.0,
    };
    let has_audio = streams
        .iter()
        .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));
    let codec = video_stream
        .get("codec_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(TemplateProbe {
        width,
        height,
        pixel_aspect,
        fps,
        duration_s,
        frames,
        has_audio,
        codec,
    })
}

/// Retourne la sonde uniquement si la vidéo est un template direct valide.
pub fn probe_direct(ffprobe: &Path, video: &Path, prefix: &str) -> Result<Option<TemplateProbe>> {
    if !matches_name(video, prefix) {
        return Ok(None);
    }
    let probe = probe(ffprobe, video)?;
    if probe.matches_profile(video) {
        Ok(Some(probe))
    } else {
        Ok(None)
    }
}

/// Dimensions d'une image fixe (logo), sans dépendre d'une bibliothèque image.
pub fn probe_image_size(ffprobe: &Path, image: &Path) -> Option<(u32, u32)> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(image)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let payload: Value = serde_json::from_slice(&output.stdout).ok()?;
    let stream = payload.get("streams")?.as_array()?.first()?;
    Some((
        stream.get("width")?.as_u64()? as u32,
        stream.get("height")?.as_u64()? as u32,
    ))
}

/// CSV STEP2 d'un seul segment couvrant toute la vidéo.
pub fn single_scene_csv_bytes(frames: u64) -> Result<Vec<u8>> {
    if frames == 0 {
        bail!("impossible de générer un CSV pour zéro frame");
    }
    let end = usize::try_from(frames - 1).context("nombre de frames trop grand")?;
    Ok(scenes_csv_text(&[(0, end)], FPS).into_bytes())
}

pub fn write_single_scene_csv(path: &Path, frames: u64) -> Result<()> {
    if frames == 0 {
        bail!("impossible de générer un CSV pour zéro frame");
    }
    let end = usize::try_from(frames - 1).context("nombre de frames trop grand")?;
    write_scenes_csv(path, &[(0, end)], FPS)
}

/// Construit le `.ecproj` brut d'un template direct.
///
/// La structure vient de l'asset mesuré sur 295 ; seuls le nom, le chemin, la
/// durée et le logo sont injectés.
pub fn raw_project(
    probe: &TemplateProbe,
    video: &Path,
    stem: &str,
    logo: Option<(&Path, (u32, u32))>,
) -> Result<Value> {
    let mut doc: Value = serde_json::from_str(include_str!(
        "../../../assets/effectcraft/sdm_raw_template.json"
    ))
    .context("asset sdm_raw_template.json invalide")?;
    let ticks = probe.duration_ticks();
    let video_name = video
        .file_name()
        .and_then(|name| name.to_str())
        .context("nom de vidéo non UTF-8")?;
    let video_path = video.to_string_lossy().to_string();
    let comp_name = format!("{stem}_9x16");
    let segment_name = format!(
        "Segment 1 (00:00:00,000 - {})",
        timecode_comma(probe.duration_s)
    );

    {
        let items = doc
            .get_mut("items")
            .and_then(Value::as_object_mut)
            .context("asset sans items")?;
        let video_item = items.get_mut("1").context("asset sans footage vidéo")?;
        video_item["name"] = json!(video_name);
        let kind = video_item
            .get_mut("kind")
            .and_then(Value::as_object_mut)
            .context("footage vidéo sans kind")?;
        kind.insert("path".into(), json!(video_path));
        kind.insert("width".into(), json!(probe.width));
        kind.insert("height".into(), json!(probe.height));
        kind.insert("duration".into(), json!(ticks));
        kind.insert("has_audio".into(), json!(probe.has_audio));
        kind.insert("has_video".into(), json!(true));
        kind.insert("frame_rate".into(), json!({"num": 25, "den": 1}));

        let comp_item = items.get_mut("2").context("asset sans composition")?;
        comp_item["name"] = json!(comp_name);
        let comp = comp_item
            .get_mut("kind")
            .and_then(Value::as_object_mut)
            .context("composition sans kind")?;
        // Le nom de composition est porté par l'item ; le format du panel n'a
        // pas de `kind.name`.
        comp.remove("name");
        comp.insert("duration".into(), json!(ticks));
        comp.insert("work_area".into(), json!([0, ticks]));
        comp.insert("frame_rate".into(), json!({"num": 25, "den": 1}));
        let layers = comp
            .get_mut("layers")
            .and_then(Value::as_array_mut)
            .context("composition sans calques")?;
        let segment = layers
            .first_mut()
            .context("composition sans segment de base")?;
        segment["name"] = json!(segment_name);
        segment["out_point"] = json!(ticks);
        segment["in_point"] = json!(0);
        segment["start_time"] = json!(0);
        segment["parent"] = Value::Null;
        segment["switches"]["audio"] = json!(probe.has_audio);
        segment["props"]["name"] = json!(video_name);

        match logo {
            Some((logo_path, (width, height))) => {
                let logo_item = items.get_mut("28").context("asset sans logo sdm.png")?;
                let kind = logo_item
                    .get_mut("kind")
                    .and_then(Value::as_object_mut)
                    .context("logo sans kind")?;
                kind.insert("path".into(), json!(logo_path.to_string_lossy()));
                kind.insert("width".into(), json!(width));
                kind.insert("height".into(), json!(height));
            }
            None => {
                items.remove("28");
            }
        }
    }

    let max_id = doc
        .get("items")
        .and_then(Value::as_object)
        .map(|items| {
            items
                .keys()
                .filter_map(|id| id.parse::<i64>().ok())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let max_uid = crate::post::ecproj::max_uid(&doc);
    doc["next_id"] = json!(max_id.max(max_uid) + 100);
    Ok(doc)
}

fn timecode_comma(seconds: f64) -> String {
    let total_millis = (seconds.max(0.0) * 1000.0).round() as u64;
    let hours = total_millis / 3_600_000;
    let minutes = (total_millis % 3_600_000) / 60_000;
    let seconds = (total_millis % 60_000) / 1000;
    let millis = total_millis % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

fn parse_fraction(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() || value == "0/0" {
        return None;
    }
    let pair = value.split_once('/').or_else(|| value.split_once(':'));
    if let Some((numerator, denominator)) = pair {
        let numerator = numerator.trim().parse::<f64>().ok()?;
        let denominator = denominator.trim().parse::<f64>().ok()?;
        (denominator != 0.0).then_some(numerator / denominator)
    } else {
        value.parse::<f64>().ok()
    }
}

/// Liste les vidéos d'un dossier `docs` (même surface que le panel).
pub fn docs_videos(docs: &Path) -> Vec<PathBuf> {
    const EXTENSIONS: &[&str] = &[
        "mp4", "mov", "avi", "mxf", "mpg", "mpeg", "wmv", "webm", "mkv",
    ];
    let Ok(entries) = std::fs::read_dir(docs) else {
        return Vec::new();
    };
    let mut videos: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    EXTENSIONS
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                })
        })
        .collect();
    videos.sort();
    videos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_scene_csv_matches_step2_reference() {
        let csv = String::from_utf8(single_scene_csv_bytes(1317).unwrap()).unwrap();
        assert_eq!(
            csv,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\r\n\
             1,00:00:00.000,00:00:52.640,1,1317\r\n"
        );
        let csv = String::from_utf8(single_scene_csv_bytes(531).unwrap()).unwrap();
        assert!(csv.ends_with("1,00:00:00.000,00:00:21.200,1,531\r\n"));
    }

    #[test]
    fn timecode_uses_comma() {
        assert_eq!(timecode_comma(21.24), "00:00:21,240");
    }

    #[test]
    fn parses_colon_and_slash_ratios() {
        assert_eq!(parse_fraction("1:1"), Some(1.0));
        assert_eq!(parse_fraction("25/1"), Some(25.0));
        let sar = parse_fraction("4:3").unwrap();
        assert!((sar - 4.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn raw_project_uses_duration_and_optional_logo() {
        let probe = TemplateProbe {
            width: 1920,
            height: 1080,
            pixel_aspect: 1.0,
            fps: 25.0,
            duration_s: 21.24,
            frames: 531,
            has_audio: true,
            codec: "h264".into(),
        };
        let doc = raw_project(
            &probe,
            Path::new("/tmp/M6 - SDM - Leslie et Léo.mp4"),
            "M6 - SDM - Leslie et Léo",
            None,
        )
        .unwrap();
        assert_eq!(doc["items"]["2"]["kind"]["duration"], 5395299840000_i64);
        assert_eq!(doc["items"]["2"]["name"], "M6 - SDM - Leslie et Léo_9x16");
        assert_eq!(doc["items"]["1"]["kind"]["has_audio"], true);
        assert!(doc["items"].get("28").is_none());
        assert_eq!(
            doc["items"]["2"]["kind"]["layers"][0]["name"],
            "Segment 1 (00:00:00,000 - 00:00:21,240)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn probe_direct_accepts_reference_profile_only() {
        let dir = tempfile::tempdir().unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let video = dir.path().join("M6 - SDM - Leslie et Léo.mp4");
        std::fs::write(&video, b"video").unwrap();
        let probe = probe_direct(&ffprobe, &video, "M6 - SDM - ")
            .unwrap()
            .expect("profil de référence");
        assert_eq!(probe.frames, 531);
        assert!((probe.duration_s - 21.24).abs() < 1e-9);
        assert!(probe_direct(&ffprobe, &video, "Autre - ")
            .unwrap()
            .is_none());
        let other = dir.path().join("M6 - SDM - Autre.mp4");
        std::fs::write(&other, b"video").unwrap();
        assert!(probe_direct(&ffprobe, &other, "M6 - SDM - ")
            .unwrap()
            .is_some());
    }

    #[test]
    fn bypass_resolves_prefixes_and_boundaries() {
        let bypass = Bypass::new("M6 - SDM - ", &["W9 - LPHDF".to_string()]);
        assert_eq!(
            bypass.mode_for_name(Path::new("M6 - SDM - Leslie et Léo.mp4")),
            Some(BypassMode::Template)
        );
        assert_eq!(
            bypass.mode_for_name(Path::new("W9 - LPHDF - La découverte.mp4")),
            Some(BypassMode::ArchiveOnly)
        );
        assert_eq!(
            bypass.mode_for_name(Path::new("W9 - LPHDF  -Un bal deux possibilités .mp4")),
            Some(BypassMode::ArchiveOnly)
        );
        assert_eq!(bypass.mode_for_name(Path::new("W9 - LPHDFX.mp4")), None);
        assert_eq!(bypass.mode_for_name(Path::new("Autre.mp4")), None);
        assert!(Bypass::default().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn probe_direct_rejects_non_square_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"4:3","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let video = dir.path().join("M6 - SDM - Anamorphique.mp4");
        std::fs::write(&video, b"video").unwrap();
        assert!(probe_direct(&ffprobe, &video, "M6 - SDM - ")
            .unwrap()
            .is_none());
    }
}
