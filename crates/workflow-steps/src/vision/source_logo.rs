//! Registre spatial des logos de programme (D-052).
//!
//! Le registre ne détecte rien : il fournit une position source déterministe
//! pour les logos de programme d'un corpus identifié. Les moteurs QA/A2
//! décident ensuite s'ils doivent confirmer ou signaler cette position.
//!
//! Les bboxes du catalogue sont demi-ouvertes, en pixels du canvas source de
//! référence (`source_resolution`, 1920×1080). Elles sont remises à l'échelle
//! de la source réelle avant usage.

use crate::vision::text::normalize_text;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const CATALOG_FILENAME: &str = "source_logos.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLogoCatalog {
    pub schema: u32,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub source: String,
    pub source_resolution: [u32; 2],
    #[serde(default)]
    pub reference_root: String,
    #[serde(default)]
    pub zones: SourceLogoZones,
    #[serde(default)]
    pub emissions: Vec<SourceLogoEmission>,
    #[serde(default)]
    pub clean_feeds: Vec<CleanFeed>,
    #[serde(default)]
    pub no_logo_emissions: Vec<NoLogoEmission>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLogoZones {
    #[serde(default)]
    pub bottom_left: Option<[i64; 4]>,
    #[serde(default)]
    pub top_left: Option<[i64; 4]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLogoEmission {
    pub name: String,
    pub channel: String,
    pub position: String,
    #[serde(default)]
    pub dossier: String,
    pub bbox: [i64; 4],
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Fichiers sources explicitement rattachés à l'émission par le CSV
    /// client. Ils priment sur les alias : « Kamila… Marseillais » est un
    /// épisode du Cross, pas des Marseillais.
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub variants: Vec<SourceLogoVariant>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLogoVariant {
    pub crop: String,
    pub bbox: [i64; 4],
    #[serde(default)]
    pub opaque_pixels: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanFeed {
    pub emission: String,
    pub channel: String,
    pub video: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoLogoEmission {
    pub name: String,
    pub channel: String,
    #[serde(default)]
    pub dossier: String,
}

/// Position résolue pour une source réelle, en pixels source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLogoPrior {
    pub name: String,
    pub channel: String,
    pub position: String,
    pub bbox: [f64; 4],
    pub variants: Vec<SourceLogoVariantPrior>,
}

/// Variante visuelle d'un logo : bbox source et crop RGBA associé, relatif à
/// la racine de références fournie par l'appelant.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLogoVariantPrior {
    pub crop: String,
    /// Bbox mise à l'échelle de la source réelle, utilisée pour la projection.
    pub bbox: [f64; 4],
    /// Bbox dans le canvas de référence 1920×1080, utilisée pour recadrer le
    /// PNG de référence.
    pub reference_bbox: [i64; 4],
}

impl SourceLogoCatalog {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("lecture du registre de logos {}", path.display()))?;
        let catalog: Self = toml::from_str(&text)
            .with_context(|| format!("registre de logos TOML invalide {}", path.display()))?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            bail!(
                "registre de logos: schéma {} non pris en charge",
                self.schema
            );
        }
        if self.source_resolution[0] == 0 || self.source_resolution[1] == 0 {
            bail!("registre de logos: résolution source nulle");
        }
        for emission in &self.emissions {
            if !matches!(emission.position.as_str(), "bottom_left" | "top_left") {
                bail!(
                    "registre de logos: position inconnue {} pour {}",
                    emission.position,
                    emission.name
                );
            }
            validate_bbox(emission.bbox, &emission.name)?;
            for variant in &emission.variants {
                validate_bbox(
                    variant.bbox,
                    &format!("{} ({})", emission.name, variant.crop),
                )?;
            }
        }
        for zone in [
            self.zones.bottom_left.as_ref(),
            self.zones.top_left.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_bbox(*zone, "zone")?;
        }
        Ok(())
    }

    /// Résout une source vers l'émission la plus spécifique. Un clean feed
    /// déclaré est prioritaire : il ne doit jamais hériter du prior d'une
    /// autre vidéo de la même émission.
    pub fn resolve_source(&self, source_name: &str) -> Option<&SourceLogoEmission> {
        let file = source_file_name(source_name);
        if self.is_clean_feed(file) {
            return None;
        }
        let source_lower = file.to_lowercase();
        let source_norm = normalize_text(file);
        let source_key = source_stem(file);
        if !source_key.is_empty() {
            for emission in &self.emissions {
                if emission
                    .sources
                    .iter()
                    .any(|candidate| source_stem(candidate) == source_key)
                {
                    return Some(emission);
                }
            }
        }
        let mut best: Option<(usize, &SourceLogoEmission)> = None;
        for emission in &self.emissions {
            for alias in &emission.aliases {
                if !alias_matches(&source_lower, &source_norm, alias) {
                    continue;
                }
                let score = normalize_text(alias).chars().count();
                if best.is_none_or(|(current, _)| score > current) {
                    best = Some((score, emission));
                }
            }
        }
        best.map(|(_, emission)| emission)
    }

    pub fn is_clean_feed(&self, source_name: &str) -> bool {
        let source = source_stem(source_name);
        self.clean_feeds.iter().any(|feed| {
            let feed_stem = source_stem(&feed.video);
            !feed_stem.is_empty() && feed_stem == source
        })
    }

    pub fn prior_for_source(
        &self,
        source_name: &str,
        source_width: u32,
        source_height: u32,
    ) -> Option<SourceLogoPrior> {
        let emission = self.resolve_source(source_name)?;
        Some(SourceLogoPrior::from_emission(
            emission,
            self.source_resolution,
            source_width,
            source_height,
        ))
    }

    /// Racine des crops de référence. Le chemin explicite de l'appelant
    /// prime sur `reference_root` du catalogue ; un catalogue versionné ne
    /// doit pas dépendre d'un chemin machine.
    pub fn reference_root(&self, override_root: Option<&Path>) -> Option<PathBuf> {
        override_root.map(Path::to_path_buf).or_else(|| {
            let root = self.reference_root.trim();
            (!root.is_empty()).then(|| PathBuf::from(root))
        })
    }

    /// Fichiers de crops référencés par le catalogue, dans l'ordre stable du
    /// fichier. Sert à l'empreinte de cache et à la validation des assets.
    pub fn reference_crops(&self, root: &Path) -> Vec<PathBuf> {
        self.emissions
            .iter()
            .flat_map(|emission| emission.variants.iter())
            .map(|variant| resolve_reference_path(root, &variant.crop))
            .collect()
    }

    /// Zones génériques utilisables quand aucune émission n'est reconnue.
    /// Elles ne produisent aucune décision, seulement un repère auditable
    /// pour la confirmation temporelle.
    pub fn generic_zones(
        &self,
        source_width: u32,
        source_height: u32,
    ) -> Vec<(&'static str, [f64; 4])> {
        let mut zones = Vec::new();
        if let Some(bbox) = self.zones.bottom_left {
            zones.push((
                "bottom_left",
                scale_bbox(bbox, self.source_resolution, source_width, source_height),
            ));
        }
        if let Some(bbox) = self.zones.top_left {
            zones.push((
                "top_left",
                scale_bbox(bbox, self.source_resolution, source_width, source_height),
            ));
        }
        zones
    }
}

impl SourceLogoPrior {
    fn from_emission(
        emission: &SourceLogoEmission,
        reference: [u32; 2],
        source_width: u32,
        source_height: u32,
    ) -> Self {
        let scale_bbox = |bbox: [i64; 4]| scale_bbox(bbox, reference, source_width, source_height);
        Self {
            name: emission.name.clone(),
            channel: emission.channel.clone(),
            position: emission.position.clone(),
            bbox: scale_bbox(emission.bbox),
            variants: emission
                .variants
                .iter()
                .map(|variant| SourceLogoVariantPrior {
                    crop: variant.crop.clone(),
                    bbox: scale_bbox(variant.bbox),
                    reference_bbox: variant.bbox,
                })
                .collect(),
        }
    }
}

/// Remet une bbox du canvas de référence à l'échelle de la source réelle.
fn scale_bbox(
    bbox: [i64; 4],
    reference: [u32; 2],
    source_width: u32,
    source_height: u32,
) -> [f64; 4] {
    let scale_x = f64::from(source_width) / f64::from(reference[0]);
    let scale_y = f64::from(source_height) / f64::from(reference[1]);
    [
        bbox[0] as f64 * scale_x,
        bbox[1] as f64 * scale_y,
        bbox[2] as f64 * scale_x,
        bbox[3] as f64 * scale_y,
    ]
}

/// Résout un crop relatif. Le chemin explicite peut pointer sur le dossier
/// `emissions` ou sur son parent `references_logos` : les deux formes sont
/// acceptées sans dupliquer la configuration.
pub fn resolve_reference_path(root: &Path, crop: &str) -> PathBuf {
    let direct = root.join(crop);
    if direct.is_file() {
        return direct;
    }
    let nested = root.join("emissions").join(crop);
    if nested.is_file() {
        nested
    } else {
        direct
    }
}

pub fn bbox_area(bbox: [f64; 4]) -> f64 {
    (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)
}

pub fn bbox_intersection(left: [f64; 4], right: [f64; 4]) -> Option<[f64; 4]> {
    let bbox = [
        left[0].max(right[0]),
        left[1].max(right[1]),
        left[2].min(right[2]),
        left[3].min(right[3]),
    ];
    (bbox[2] > bbox[0] && bbox[3] > bbox[1]).then_some(bbox)
}

pub fn bbox_intersection_area(left: [f64; 4], right: [f64; 4]) -> f64 {
    bbox_intersection(left, right).map(bbox_area).unwrap_or(0.0)
}

/// Part du plus petit rectangle couverte par l'intersection.
pub fn bbox_overlap_ratio(left: [f64; 4], right: [f64; 4]) -> f64 {
    let smallest = bbox_area(left).min(bbox_area(right));
    if smallest <= 0.0 {
        return 0.0;
    }
    bbox_intersection_area(left, right) / smallest
}

pub fn clip_bbox(bbox: [f64; 4], width: u32, height: u32) -> Option<[f64; 4]> {
    let clipped = [
        bbox[0].max(0.0),
        bbox[1].max(0.0),
        bbox[2].min(f64::from(width)),
        bbox[3].min(f64::from(height)),
    ];
    (clipped[2] > clipped[0] && clipped[3] > clipped[1]).then_some(clipped)
}

fn validate_bbox(bbox: [i64; 4], label: &str) -> Result<()> {
    if bbox[2] <= bbox[0] || bbox[3] <= bbox[1] {
        bail!("registre de logos: bbox invalide pour {label}: {bbox:?}");
    }
    Ok(())
}

fn source_file_name(source_name: &str) -> &str {
    Path::new(source_name.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source_name)
}

fn source_stem(source_name: &str) -> String {
    normalize_text(
        Path::new(source_name.trim())
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(source_name),
    )
}

fn alias_matches(source_lower: &str, source_norm: &str, alias: &str) -> bool {
    let alias = alias.trim();
    if alias.is_empty() {
        return false;
    }
    if alias.ends_with('-') {
        let prefix = alias.trim_end_matches('-').trim_end().to_lowercase();
        !prefix.is_empty() && source_lower.starts_with(&prefix)
    } else {
        let normalized = normalize_text(alias);
        !normalized.is_empty() && source_norm.contains(&normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"
schema = 1
source_resolution = [1920, 1080]

[zones]
bottom_left = [61, 719, 497, 1061]
top_left = [87, 48, 445, 112]

[[emissions]]
name = "66 Minutes"
channel = "M6"
position = "bottom_left"
bbox = [91, 846, 298, 1026]
aliases = ["66M -", "66 minutes"]
sources = ["66M - ABHARTACH.mp4"]

[[emissions.variants]]
crop = "66_Minutes/crop.png"
bbox = [91, 846, 298, 1026]
opaque_pixels = 37260

[[emissions]]
name = "Les Marseillais"
channel = "W9"
position = "bottom_left"
bbox = [97, 719, 437, 1021]
aliases = ["Marseillais"]
sources = ["Les Marseillais s’envolent sur une bouée.mp4"]

[[emissions]]
name = "Le Cross"
channel = "W9"
position = "bottom_left"
bbox = [97, 719, 437, 1021]
aliases = ["Le Cross"]
sources = ["Kamila ne sait pas écrire Marseillais !.mp4", "Le RDM remporte cette saison du cross !.mp4"]

[[emissions]]
name = "Top Chef"
channel = "M6"
position = "bottom_left"
bbox = [87, 903, 387, 1028]
aliases = ["M6 - Top Chef -", "Top Chef"]
sources = ["M6 - Top Chef - Ses desserts craquent.mp4"]

[[clean_feeds]]
emission = "Top Chef"
channel = "M6"
video = "M6 - Top Chef - Ce candidat est obligé d’abandonner Top Chef.mp4"

[[no_logo_emissions]]
name = "Turbo"
channel = "M6"
"#;

    fn catalog() -> SourceLogoCatalog {
        toml::from_str(CATALOG).unwrap()
    }

    #[test]
    fn resolves_prefix_alias_and_scales_bbox() {
        let catalog = catalog();
        let prior = catalog
            .prior_for_source("66M - VENTES.mp4", 1280, 720)
            .unwrap();
        assert_eq!(prior.name, "66 Minutes");
        assert_eq!(prior.position, "bottom_left");
        assert!((prior.bbox[0] - 91.0 * 1280.0 / 1920.0).abs() < 1e-9);
        assert!((prior.bbox[3] - 1026.0 * 720.0 / 1080.0).abs() < 1e-9);
        assert_eq!(prior.variants.len(), 1);
        assert_eq!(prior.variants[0].crop, "66_Minutes/crop.png");
        assert_eq!(prior.variants[0].reference_bbox, [91, 846, 298, 1026]);
    }

    #[test]
    fn resolves_longest_alias_and_ignores_clean_feed_example() {
        let catalog = catalog();
        assert_eq!(
            catalog
                .resolve_source("M6 - Top Chef - Ses desserts craquent.mp4")
                .unwrap()
                .name,
            "Top Chef"
        );
        assert!(catalog
            .resolve_source("M6 - Top Chef - Ce candidat est obligé d’abandonner Top Chef.mp4")
            .is_none());
        assert!(catalog
            .is_clean_feed("M6 - Top Chef - Ce candidat est obligé d’abandonner Top Chef.mp4"));
    }

    #[test]
    fn unknown_source_has_no_prior() {
        assert!(catalog()
            .resolve_source("Son travail empiète sur sa vie de famille.mp4")
            .is_none());
    }

    #[test]
    fn explicit_source_wins_over_an_ambiguous_alias() {
        let catalog = catalog();
        assert_eq!(
            catalog
                .resolve_source("Kamila ne sait pas écrire Marseillais !.mp4")
                .unwrap()
                .name,
            "Le Cross"
        );
        assert_eq!(
            catalog
                .resolve_source("Les Marseillais s’envolent sur une bouée.mp4")
                .unwrap()
                .name,
            "Les Marseillais"
        );
    }

    #[test]
    fn reference_path_accepts_the_parent_folder() {
        let dir = tempfile::tempdir().unwrap();
        let crop = dir
            .path()
            .join("emissions")
            .join("66_Minutes")
            .join("crop.png");
        std::fs::create_dir_all(crop.parent().unwrap()).unwrap();
        std::fs::write(&crop, b"png").unwrap();
        assert_eq!(
            resolve_reference_path(dir.path(), "66_Minutes/crop.png"),
            crop
        );
    }

    #[test]
    fn generic_zones_scale_with_the_source() {
        let zones = catalog().generic_zones(1280, 720);
        assert_eq!(zones.len(), 2);
        assert_eq!(zones[0].0, "bottom_left");
        assert!((zones[0].1[0] - 61.0 * 1280.0 / 1920.0).abs() < 1e-9);
        assert!((zones[1].1[3] - 112.0 * 720.0 / 1080.0).abs() < 1e-9);
    }

    #[test]
    fn bbox_helpers_use_the_smallest_rectangle() {
        let logo = [0.0, 0.0, 100.0, 100.0];
        let candidate = [50.0, 50.0, 300.0, 300.0];
        assert!((bbox_intersection_area(logo, candidate) - 2500.0).abs() < 1e-9);
        assert!((bbox_overlap_ratio(logo, candidate) - 0.25).abs() < 1e-9);
        assert_eq!(
            clip_bbox([-10.0, -10.0, 1200.0, 2000.0], 1080, 1920),
            Some([0.0, 0.0, 1080.0, 1920.0])
        );
    }

    fn repository_catalog() -> SourceLogoCatalog {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/qa/source_logos.toml");
        SourceLogoCatalog::load(&path).unwrap()
    }

    #[test]
    fn repository_catalog_is_valid_and_complete() {
        // Given: le catalogue versionné livré avec le dépôt.
        let catalog = repository_catalog();

        // When/Then: les comptes et les deux zones sont complets.
        assert_eq!(catalog.emissions.len(), 30);
        assert_eq!(catalog.clean_feeds.len(), 21);
        assert_eq!(catalog.no_logo_emissions.len(), 9);
        assert!(catalog.zones.bottom_left.is_some());
        assert!(catalog.zones.top_left.is_some());
    }

    #[test]
    fn resolves_new_lrdlr_source_with_bottom_left_prior() {
        // Given: le catalogue versionné et le fichier source exact du CSV.
        let catalog = repository_catalog();
        let source = "6ter - LRDLR - Au volant de son camion, elle s’engage sur une route interdite aux poids lourds .mp4";

        // When: la source est résolue puis projetée sur le canvas de référence.
        let emission = catalog.resolve_source(source).unwrap();
        let prior = catalog.prior_for_source(source, 1920, 1080).unwrap();

        // Then: la source exacte prime et le prior reste en bas-gauche.
        assert_eq!(emission.name, "Les Reines de la Route");
        assert!(emission.sources.iter().any(|candidate| candidate == source));
        assert_eq!(prior.name, "Les Reines de la Route");
        assert_eq!(prior.channel, "6ter");
        assert_eq!(prior.position, "bottom_left");
        assert_eq!(prior.bbox, [99.0, 848.0, 315.0, 1022.0]);
        assert_eq!(prior.variants.len(), 1);
        assert_eq!(prior.variants[0].crop, "Les_Reines_de_la_Route/crop.png");
        assert_eq!(prior.variants[0].reference_bbox, [99, 848, 315, 1022]);

        let zone = catalog.zones.bottom_left.unwrap();
        assert!(prior.bbox[0] >= zone[0] as f64);
        assert!(prior.bbox[1] >= zone[1] as f64);
        assert!(prior.bbox[2] <= zone[2] as f64);
        assert!(prior.bbox[3] <= zone[3] as f64);
    }

    #[test]
    fn new_wanted_clean_feed_has_no_prior() {
        // Given: le catalogue versionné et un clean feed WANTED du CSV.
        let catalog = repository_catalog();
        let source = "WANTED - SOIREE EM5 rs1.mp4";

        // When/Then: le clean feed est reconnu et n'hérite d'aucun prior.
        assert!(catalog.is_clean_feed(source));
        assert!(catalog.resolve_source(source).is_none());
    }
}
