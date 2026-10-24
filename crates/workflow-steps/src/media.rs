//! Filtres média partagés par STEP1 à STEP4.
//!
//! La détection des `.mov` à conserver (alpha ou nom configuré) était
//! dupliquée dans chaque étape ; une divergence entre copies changeait
//! silencieusement la liste des médias traités. Ce module est désormais la
//! seule implémentation.

use std::path::Path;
use std::process::Command;

const ALPHA_PIX_FMT_PREFIXES: [&str; 9] = [
    "yuva", "gbrap", "ya8", "ya16", "ayuv", "rgba", "bgra", "argb", "abgr",
];
const ALPHA_CODEC_TAGS: [&str; 4] = ["ap4h", "ap4x", "rle", "png"];

pub fn pix_fmt_has_alpha(pix_fmt: &str) -> bool {
    let normalized = pix_fmt.trim().to_ascii_lowercase();
    !normalized.is_empty()
        && ALPHA_PIX_FMT_PREFIXES
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
}

/// Détection conservatrice : un `.mov` dont le codec est indéterminé est
/// préservé. Un probe en échec renvoie `None`, que l'appelant convertit en
/// conservation.
pub fn probe_mov_alpha(path: &Path, ffprobe: &Path) -> Option<bool> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=codec_tag_string,pix_fmt",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let streams = payload.get("streams")?.as_array()?;
    if streams.is_empty() {
        return None;
    }
    let mut missing_pix_fmt = false;
    for stream in streams {
        match stream.get("pix_fmt").and_then(|value| value.as_str()) {
            Some(pix_fmt) if !pix_fmt.trim().is_empty() => {
                if pix_fmt_has_alpha(pix_fmt) {
                    return Some(true);
                }
            }
            _ => missing_pix_fmt = true,
        }
    }
    if !missing_pix_fmt {
        return Some(false);
    }
    for stream in streams {
        if stream
            .get("pix_fmt")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }
        let tag = stream
            .get("codec_tag_string")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if ALPHA_CODEC_TAGS.contains(&tag.as_str()) {
            return Some(true);
        }
    }
    None
}

/// `.mov` préservé si son nom figure dans la liste configurée ou si le probe
/// détecte de l'alpha. Le filtre est inactif quand `skip_preserved_mov` est
/// faux.
pub fn is_preserved_media_with(
    path: &Path,
    skip_preserved_mov: bool,
    preserved_mov_names: &[String],
    ffprobe: &Path,
) -> bool {
    if !skip_preserved_mov {
        return false;
    }
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mov"))
    {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if preserved_mov_names
        .iter()
        .any(|preserved| preserved.eq_ignore_ascii_case(name))
    {
        return true;
    }
    probe_mov_alpha(path, ffprobe).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn given_alpha_pixel_formats_when_checked_then_conservative() {
        assert!(pix_fmt_has_alpha("yuva420p"));
        assert!(pix_fmt_has_alpha("rgba64le"));
        assert!(pix_fmt_has_alpha("ya8"));
        assert!(!pix_fmt_has_alpha("yuv420p"));
        assert!(!pix_fmt_has_alpha("gray"));
        assert!(!pix_fmt_has_alpha("pal8"));
        assert!(!pix_fmt_has_alpha(""));
    }

    #[test]
    fn given_configured_name_when_filtering_then_case_insensitive() {
        assert!(is_preserved_media_with(
            Path::new("M6+.MOV"),
            true,
            &["m6+.mov".into()],
            Path::new("/nonexistent/ffprobe")
        ));
        assert!(!is_preserved_media_with(
            Path::new("clip.mp4"),
            true,
            &["clip.mp4".into()],
            Path::new("/nonexistent/ffprobe")
        ));
        assert!(!is_preserved_media_with(
            Path::new("M6+.MOV"),
            false,
            &["m6+.mov".into()],
            Path::new("/nonexistent/ffprobe")
        ));
    }

    #[test]
    fn given_unreadable_probe_when_filtering_then_preserved() {
        let _ = PathBuf::new();
        assert!(is_preserved_media_with(
            Path::new("inconnu.mov"),
            true,
            &[],
            Path::new("/nonexistent/ffprobe")
        ));
    }
}
