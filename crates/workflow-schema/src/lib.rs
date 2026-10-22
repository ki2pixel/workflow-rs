//! Schémas et constantes partagés par les étapes Rust.

pub mod ae;
pub mod audio;
pub mod round;
pub mod tracking;

pub const AUDIO_SUFFIX: &str = "_audio.json";
pub const TRACKING_SUFFIX: &str = "_tracking.json";
pub const AE_SUFFIX: &str = "_ae.json";

pub const VIDEO_EXTENSIONS: &[&str] = &[".mp4", ".mov", ".avi", ".mkv", ".webm"];
