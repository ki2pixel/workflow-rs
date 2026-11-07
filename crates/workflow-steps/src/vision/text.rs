//! Normalisation de texte et similarité de boîtes, partagées par les
//! orchestrateurs QA (sous-titres, logos) et A2.

/// Clé de persistance : minuscules, caractères alphanumériques seulement.
pub(crate) fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn max_edits(left: &str, right: &str) -> usize {
    let shortest = left.chars().count().min(right.chars().count());
    if shortest <= 4 {
        1
    } else {
        2
    }
}

pub(crate) fn text_similarity(left: &str, right: &str) -> f64 {
    let longest = left.chars().count().max(right.chars().count());
    if longest == 0 {
        return 1.0;
    }
    1.0 - text_distance(left, right) as f64 / longest as f64
}

pub(crate) fn text_distance(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0usize; right.len() + 1];
    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(left_char != right_char);
            current[j + 1] = (current[j] + 1).min(previous[j + 1] + 1).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// Même zone : une boîte contenue dans l'autre, un IoU suffisant, ou des
/// centres proches avec une taille comparable. La containment couvre les
/// recadrages OCR d'un même logo (« e=m6 », « =m6 », « m6 »).
pub(crate) fn bbox_similarity(left: [i64; 4], right: [i64; 4]) -> bool {
    if bbox_contains(left, right) || bbox_contains(right, left) {
        return true;
    }
    let area_left = (left[2].max(1) * left[3].max(1)) as f64;
    let area_right = (right[2].max(1) * right[3].max(1)) as f64;
    let intersection_width = (left[0] + left[2]).min(right[0] + right[2]) - left[0].max(right[0]);
    let intersection_height = (left[1] + left[3]).min(right[1] + right[3]) - left[1].max(right[1]);
    let intersection = (intersection_width.max(0) * intersection_height.max(0)) as f64;
    let union = area_left + area_right - intersection;
    if union > 0.0 && intersection / union >= 0.3 {
        return true;
    }
    let center_left = (
        left[0] as f64 + left[2] as f64 / 2.0,
        left[1] as f64 + left[3] as f64 / 2.0,
    );
    let center_right = (
        right[0] as f64 + right[2] as f64 / 2.0,
        right[1] as f64 + right[3] as f64 / 2.0,
    );
    let distance = ((center_left.0 - center_right.0).powi(2)
        + (center_left.1 - center_right.1).powi(2))
    .sqrt();
    let scale = left[2].max(left[3]).max(right[2]).max(right[3]) as f64;
    let ratio = area_left / area_right;
    distance <= 0.25 * scale && (0.5..=2.0).contains(&ratio)
}

pub(crate) fn bbox_contains(outer: [i64; 4], inner: [i64; 4]) -> bool {
    const MARGIN: i64 = 2;
    inner[0] >= outer[0] - MARGIN
        && inner[1] >= outer[1] - MARGIN
        && inner[0] + inner[2] <= outer[0] + outer[2] + MARGIN
        && inner[1] + inner[3] <= outer[1] + outer[3] + MARGIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_keeps_alphanumerics_and_lowercases() {
        assert_eq!(normalize_text("LES 100"), "les100");
        assert_eq!(normalize_text("É=M6"), "ém6");
    }

    #[test]
    fn containment_and_overlap_are_similar() {
        let outer = [10, 10, 100, 40];
        assert!(bbox_similarity(outer, [20, 15, 40, 20]));
        assert!(bbox_similarity([0, 0, 50, 50], [0, 0, 50, 50]));
        assert!(!bbox_similarity([0, 0, 50, 20], [0, 100, 50, 20]));
    }
}
