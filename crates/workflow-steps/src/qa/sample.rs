//! Échantillonnage temporel partagé par les contrôles Stage 1.
//!
//! Le visage et l'OCR doivent regarder les mêmes instants : le milieu de
//! chaque segment `.ecproj` (ou 10/50/90 % avec `per_segment = 3`). Sans
//! projet, la vidéo est découpée en fenêtres de 5 s. La déduplication et le
//! tri sont identiques au PoC Python pour que les deux contrôles restent
//! comparables.

use workflow_schema::round::round_to;

#[derive(Debug, Clone)]
pub struct SegmentSpan {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct SampleTime {
    pub(crate) segment: String,
    pub(crate) time: f64,
}

pub(crate) fn sample_times(
    duration_s: f64,
    segments: &[SegmentSpan],
    per_segment: usize,
) -> Vec<SampleTime> {
    let fractions: &[f64] = if per_segment == 3 {
        &[0.1, 0.5, 0.9]
    } else {
        &[0.5]
    };
    let mut windows = if segments.is_empty() {
        let mut windows = Vec::new();
        let mut start = 0.0;
        while start < duration_s {
            windows.push(SegmentSpan {
                name: format!("win_{start:.0}"),
                start,
                end: (start + 5.0).min(duration_s),
            });
            start += 5.0;
        }
        windows
    } else {
        segments.to_vec()
    };
    windows.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then(left.end.total_cmp(&right.end))
            .then(left.name.cmp(&right.name))
    });

    let mut samples = Vec::new();
    let mut seen = Vec::new();
    for segment in windows {
        if segment.end <= segment.start {
            continue;
        }
        for fraction in fractions {
            let time = segment.start + (segment.end - segment.start) * fraction;
            if !(0.0..duration_s).contains(&time) {
                continue;
            }
            let key = round_to(time, 2);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            samples.push(SampleTime {
                segment: segment.name.clone(),
                time: round_to(time, 3),
            });
        }
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_three_points_per_segment_and_deduplicates() {
        let segments = vec![SegmentSpan {
            name: "Segment 1".into(),
            start: 0.0,
            end: 10.0,
        }];
        let samples = sample_times(10.0, &segments, 3);
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].time, 1.0);
        assert_eq!(samples[1].time, 5.0);
        assert_eq!(samples[2].time, 9.0);
    }

    #[test]
    fn samples_fall_back_to_five_second_windows() {
        let samples = sample_times(12.0, &[], 1);
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].time, 2.5);
        assert_eq!(samples[2].time, 11.0);
    }

    #[test]
    fn samples_sort_segments_before_sampling() {
        let segments = vec![
            SegmentSpan {
                name: "B".into(),
                start: 10.0,
                end: 20.0,
            },
            SegmentSpan {
                name: "A".into(),
                start: 0.0,
                end: 10.0,
            },
        ];
        let samples = sample_times(20.0, &segments, 1);
        assert_eq!(samples[0].segment, "A");
        assert_eq!(samples[1].segment, "B");
    }
}
