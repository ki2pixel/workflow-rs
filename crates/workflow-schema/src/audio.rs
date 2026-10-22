use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const AUDIO_FRAMES_KEY: &str = "frames_analysis";
pub const FPS_KEY: &str = "fps";
pub const TOTAL_FRAMES_KEY: &str = "total_frames";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioMeta {
    pub fps: Option<f64>,
    pub total_frames: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpeakerStats {
    pub unique_speakers: Vec<String>,
    pub speaker_frame_counts: BTreeMap<String, u64>,
}
