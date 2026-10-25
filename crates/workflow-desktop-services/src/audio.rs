//! Lecture des sons du workflow.
//!
//! Le frontend legacy joue ses sons dans la WebView ; côté hôte, la lecture
//! est opt-in et passe par `rodio` (MP3 via Symphonia). Le dossier des sons
//! reste celui du serveur (`assets/web/sound-design` par défaut).

use crate::error::ServicesError;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    WorkflowStart,
    CheckboxInteraction,
    WorkflowCompletion,
    ErrorEvent,
    AutoModeToggle,
    StepSuccess,
    CsvDownloadInitiation,
    CsvDownloadCompletion,
}

impl SoundEvent {
    pub fn file_name(self) -> &'static str {
        match self {
            Self::WorkflowStart => "DM_20250703182122_001.mp3",
            Self::CheckboxInteraction => "DM_20250703182143_001.mp3",
            Self::WorkflowCompletion => "DM_20250703182159_001.mp3",
            Self::ErrorEvent => "DM_20250703182219_001.mp3",
            Self::AutoModeToggle => "DM_20250703182234_001.mp3",
            Self::StepSuccess => "DM_20250703182315_001.mp3",
            Self::CsvDownloadInitiation => "DM_20250703194305_001.mp3",
            Self::CsvDownloadCompletion => "DM_20250703182159_001.mp3",
        }
    }
}

pub trait AudioPlayer: Send + Sync {
    fn play(&self, event: SoundEvent) -> Result<(), ServicesError>;
}

#[derive(Debug, Default)]
pub struct NoopAudio;

impl AudioPlayer for NoopAudio {
    fn play(&self, _event: SoundEvent) -> Result<(), ServicesError> {
        Ok(())
    }
}

/// Lecteur MP3 local, sans dépendance à la WebView.
#[derive(Debug, Clone)]
pub struct RodioAudio {
    dir: PathBuf,
}

impl RodioAudio {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl AudioPlayer for RodioAudio {
    fn play(&self, event: SoundEvent) -> Result<(), ServicesError> {
        let path = self.dir.join(event.file_name());
        let file = std::fs::File::open(&path).map_err(|error| {
            ServicesError::Config(format!("son introuvable {}: {error}", path.display()))
        })?;
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|error| ServicesError::Config(format!("sortie audio: {error}")))?;
        sink.log_on_drop(false);
        let player = rodio::Player::connect_new(sink.mixer());
        let decoder = rodio::Decoder::new(std::io::BufReader::new(file))
            .map_err(|error| ServicesError::Config(format!("décodage MP3: {error}")))?;
        player.append(decoder);
        player.sleep_until_end();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_legacy_sound_events() {
        for event in [
            SoundEvent::WorkflowStart,
            SoundEvent::CheckboxInteraction,
            SoundEvent::WorkflowCompletion,
            SoundEvent::ErrorEvent,
            SoundEvent::AutoModeToggle,
            SoundEvent::StepSuccess,
            SoundEvent::CsvDownloadInitiation,
            SoundEvent::CsvDownloadCompletion,
        ] {
            assert!(event.file_name().ends_with(".mp3"));
        }
    }

    #[test]
    fn rodio_audio_reports_missing_file_before_opening_device() {
        let player = RodioAudio::new(std::path::PathBuf::from("/nonexistent/sound-design"));
        let error = player.play(SoundEvent::WorkflowStart).unwrap_err();
        assert!(matches!(error, ServicesError::Config(_)));
    }
}
