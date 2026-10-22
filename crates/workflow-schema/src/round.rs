/// Arrondi décimal compatible avec `round(x, n)` de Python.
///
/// L'implémentation passe par le formatage décimal de la valeur binaire, comme
/// Python, au lieu de multiplier puis d'arrondir. Cela évite les écarts sur les
/// demi-valeurs (par exemple `round(0.60005, 4) == 0.6`).
pub fn round_to(value: f64, decimals: u32) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let decimals = decimals as usize;
    format!("{value:.decimals$}")
        .parse::<f64>()
        .unwrap_or(value)
}

pub fn round2(value: f64) -> f64 {
    round_to(value, 2)
}

pub fn round4(value: f64) -> f64 {
    round_to(value, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_common_values() {
        assert!((round4(0.123_456) - 0.123_5).abs() < 1e-12);
        assert!((round2(123.456) - 123.46).abs() < 1e-12);
    }

    #[test]
    fn matches_python_half_cases() {
        assert_eq!(round4(0.60005), 0.6);
        assert_eq!(round4(0.45125), 0.4512);
        assert_eq!(round4(0.79675), 0.7967);
        assert_eq!(round2(2919.125), 2919.12);
        assert_eq!(round2(2.675), 2.67);
    }
}
