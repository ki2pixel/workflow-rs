/// Arrondi décimal utilisé par les agrégats STEP5/STEP6.
///
/// Python `round(x, n)` et Rust ne suivent pas exactement la même règle sur les
/// demi-valeurs. Les valeurs de production ne sont pas des demi-exacts ; ce
/// helper reproduit l'arrondi le plus proche et sera ajusté si un cas de parité
/// l'exige.
pub fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
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
}
