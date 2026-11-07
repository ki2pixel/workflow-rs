//! Moteurs de vision partagés par le QA post-export et le post-traitement A2.
//!
//! Le module est neutre : il ne connaît ni les findings QA, ni les décisions
//! A2. Il expose l'extraction de frames, l'initialisation ONNX Runtime, les
//! moteurs PP-OCRv6 et DINOv2, ainsi que les utilitaires de texte et de
//! boîtes. `qa` et `post` orchestrent ces briques avec leurs politiques
//! propres.

use serde::{Deserialize, Serialize};

pub mod dino;
pub mod frame;
pub mod hash;
pub mod inference;
pub mod ocr;
pub mod text;

/// Modèle ONNX et provenance publiée dans les rapports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub sha256: String,
    pub license: String,
}
