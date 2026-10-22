//! Diarisation AHC de STEP3.
//!
//! La production utilise une normalisation L2, une distance cosinus condensée,
//! un linkage average (`fastcluster`/`scipy`) puis `fcluster` avec un seuil
//! strict. Les locuteurs mineurs (moins de 7 s de parole contiguë) sont
//! réassignés au centroïde majeur le plus proche. Les identifiants finaux sont
//! ordonnés par durée de parole décroissante.

use anyhow::{bail, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub struct ClusterConfig {
    pub threshold: f64,
    pub min_speaker_speech_duration: f64,
    #[allow(dead_code)]
    pub max_speakers: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiarizationSegment {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
}

pub fn diarize(
    embeddings: &[Vec<f32>],
    segments: &[[f64; 2]],
    config: ClusterConfig,
) -> Result<Vec<DiarizationSegment>> {
    if embeddings.len() != segments.len() {
        bail!(
            "diarisation: {} embeddings pour {} segments",
            embeddings.len(),
            segments.len()
        );
    }
    if embeddings.is_empty() {
        return Ok(Vec::new());
    }
    if embeddings.len() == 1 {
        return Ok(vec![DiarizationSegment {
            speaker: "SPEAKER_00".to_string(),
            start: segments[0][0],
            end: segments[0][1],
        }]);
    }

    let normalized = normalize(embeddings);
    let mut labels = ahc_labels(&normalized, config.threshold);
    let mut durations = actual_durations(&labels, segments);

    let mut major: Vec<usize> = durations
        .iter()
        .filter(|(_, duration)| **duration >= config.min_speaker_speech_duration)
        .map(|(label, _)| *label)
        .collect();
    if major.is_empty() {
        // Équivalent de `max(cluster_durations, key=...)`, avec l'ordre
        // d'insertion des labels comme départage stable.
        if let Some((label, _)) =
            durations
                .iter()
                .max_by(|(left_label, left), (right_label, right)| {
                    left.total_cmp(right)
                        .then_with(|| right_label.cmp(left_label))
                })
        {
            major.push(*label);
        }
    }
    major.sort_unstable();

    let all_labels: Vec<usize> = durations.keys().copied().collect();
    if major.len() < all_labels.len() {
        let centroids: Vec<Vec<f32>> = major
            .iter()
            .map(|label| centroid(&normalized, &labels, *label))
            .collect();
        let major_set: std::collections::BTreeSet<usize> = major.iter().copied().collect();
        for (index, embedding) in normalized.iter().enumerate() {
            if major_set.contains(&labels[index]) {
                continue;
            }
            let mut best = 0usize;
            let mut best_score = f32::NEG_INFINITY;
            for (centroid_index, centroid) in centroids.iter().enumerate() {
                let score = dot(embedding, centroid);
                if score > best_score {
                    best_score = score;
                    best = centroid_index;
                }
            }
            labels[index] = major[best];
        }
        durations = actual_durations(&labels, segments);
    }

    let mut unique: Vec<usize> = durations.keys().copied().collect();
    unique.sort_by(|left, right| {
        durations[right]
            .total_cmp(&durations[left])
            .then_with(|| left.cmp(right))
    });
    let label_map: BTreeMap<usize, String> = unique
        .iter()
        .enumerate()
        .map(|(index, label)| (*label, format!("SPEAKER_{index:02}")))
        .collect();

    let mut result = Vec::new();
    let mut current_speaker = label_map[&labels[0]].clone();
    let mut current_start = segments[0][0];
    let mut current_end = segments[0][1];
    for index in 1..segments.len() {
        let speaker = label_map[&labels[index]].clone();
        if speaker == current_speaker && segments[index][0] - current_end < 1.0 {
            current_end = current_end.max(segments[index][1]);
        } else {
            result.push(DiarizationSegment {
                speaker: current_speaker,
                start: current_start,
                end: current_end,
            });
            current_speaker = speaker;
            current_start = segments[index][0];
            current_end = segments[index][1];
        }
    }
    result.push(DiarizationSegment {
        speaker: current_speaker,
        start: current_start,
        end: current_end,
    });
    Ok(result)
}

fn normalize(embeddings: &[Vec<f32>]) -> Vec<Vec<f32>> {
    embeddings
        .iter()
        .map(|embedding| {
            let norm = embedding
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            if norm > 1e-10 {
                embedding.iter().map(|value| value / norm).collect()
            } else {
                embedding.clone()
            }
        })
        .collect()
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| *a * *b)
        .sum::<f32>()
}

fn centroid(embeddings: &[Vec<f32>], labels: &[usize], label: usize) -> Vec<f32> {
    let dimensions = embeddings.first().map_or(0, Vec::len);
    let mut centroid = vec![0.0_f32; dimensions];
    let mut count = 0usize;
    for (index, embedding) in embeddings.iter().enumerate() {
        if labels[index] != label {
            continue;
        }
        for (slot, value) in centroid.iter_mut().zip(embedding.iter()) {
            *slot += *value;
        }
        count += 1;
    }
    if count > 0 {
        for value in &mut centroid {
            *value /= count as f32;
        }
    }
    let norm = centroid
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > 1e-10 {
        for value in &mut centroid {
            *value /= norm;
        }
    }
    centroid
}

/// AHC average non pondérée sur la matrice de distances cosinus complète.
///
/// Les VIDÉOS de production ont quelques centaines de fenêtres de parole ;
/// l'implémentation privilégie la fidélité au linkage de SciPy plutôt qu'un
/// algorithme approximatif. La matrice est symétrique et n'est jamais exposée.
fn ahc_labels(normalized: &[Vec<f32>], threshold: f64) -> Vec<usize> {
    let n = normalized.len();
    let mut distances = vec![0.0_f64; n * n];
    for left in 0..n {
        for right in left + 1..n {
            let similarity = dot(&normalized[left], &normalized[right]);
            let distance = 1.0 - f64::from(similarity);
            distances[left * n + right] = distance;
            distances[right * n + left] = distance;
        }
    }

    let cutoff = previous_float(threshold);
    let mut active = vec![true; n];
    let mut sizes = vec![1usize; n];
    let mut parent: Vec<usize> = (0..n).collect();
    let mut active_count = n;

    while active_count > 1 {
        let mut best = f64::INFINITY;
        let mut best_pair = None;
        for left in 0..n {
            if !active[left] {
                continue;
            }
            for right in left + 1..n {
                if !active[right] {
                    continue;
                }
                let distance = distances[left * n + right];
                if distance < best {
                    best = distance;
                    best_pair = Some((left, right));
                }
            }
        }
        let Some((left, right)) = best_pair else {
            break;
        };
        if best > cutoff {
            break;
        }
        let left_size = sizes[left];
        let right_size = sizes[right];
        let total = left_size + right_size;
        for other in 0..n {
            if !active[other] || other == left || other == right {
                continue;
            }
            let updated = (left_size as f64 * distances[left * n + other]
                + right_size as f64 * distances[right * n + other])
                / total as f64;
            distances[left * n + other] = updated;
            distances[other * n + left] = updated;
        }
        sizes[left] = total;
        active[right] = false;
        parent[right] = left;
        active_count -= 1;
    }

    (0..n).map(|index| find(&mut parent, index)).collect()
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

fn previous_float(value: f64) -> f64 {
    if value > 0.0 {
        f64::from_bits(value.to_bits() - 1)
    } else if value < 0.0 {
        f64::from_bits(value.to_bits() + 1)
    } else {
        value
    }
}

fn actual_durations(labels: &[usize], segments: &[[f64; 2]]) -> BTreeMap<usize, f64> {
    let mut grouped: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
    for (label, segment) in labels.iter().zip(segments.iter()) {
        grouped
            .entry(*label)
            .or_default()
            .push((segment[0], segment[1]));
    }
    let mut durations = BTreeMap::new();
    for (label, mut items) in grouped {
        items.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut total = 0.0;
        let mut current_start = items[0].0;
        let mut current_end = items[0].1;
        for item in items.iter().skip(1) {
            if item.0 - current_end < 1.0 {
                current_end = current_end.max(item.1);
            } else {
                total += current_end - current_start;
                current_start = item.0;
                current_end = item.1;
            }
        }
        total += current_end - current_start;
        durations.insert(label, total);
    }
    durations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_separated_embeddings_become_two_speakers() {
        let embeddings = vec![vec![1.0, 0.0], vec![0.99, 0.01], vec![0.0, 1.0]];
        let segments = vec![[0.0, 0.96], [0.48, 1.44], [1.0, 1.96]];
        let result = diarize(
            &embeddings,
            &segments,
            ClusterConfig {
                threshold: 0.32,
                min_speaker_speech_duration: 0.0,
                max_speakers: 5,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].speaker, "SPEAKER_00");
        assert_eq!(result[1].speaker, "SPEAKER_01");
    }

    #[test]
    fn duration_is_computed_on_merged_segments() {
        let labels = vec![0, 0, 1];
        let segments = vec![[0.0, 0.96], [0.48, 1.44], [1.0, 1.96]];
        let durations = actual_durations(&labels, &segments);
        assert!((durations[&0] - 1.44).abs() < 1e-9);
        assert!((durations[&1] - 0.96).abs() < 1e-9);
    }

    #[test]
    fn previous_float_is_strictly_below() {
        assert!(previous_float(0.32) < 0.32);
        assert_eq!(previous_float(0.0), 0.0);
    }

    #[test]
    fn empty_and_single_segments_are_supported() {
        let config = ClusterConfig {
            threshold: 0.32,
            min_speaker_speech_duration: 7.0,
            max_speakers: 5,
        };
        assert!(diarize(&[], &[], config).unwrap().is_empty());
        let single = diarize(&[vec![1.0, 0.0]], &[[1.0, 2.0]], config).unwrap();
        assert_eq!(single[0].speaker, "SPEAKER_00");
        assert_eq!(single[0].start, 1.0);
    }
}
