//! Filtre One-Euro appliqué aux 52 blendshapes ARKit.
//!
//! Reproduction de `utils/one_euro_filter.py` : les calculs sont en float64,
//! l'état est mis à jour uniquement sur les frames où un visage est détecté.

#[derive(Debug, Clone)]
pub struct OneEuroFilter {
    rate: f64,
    min_cutoff: f64,
    beta: f64,
    d_cutoff: f64,
    x_prev: [f64; 52],
    dx_prev: [f64; 52],
    initialized: bool,
}

impl OneEuroFilter {
    pub fn new(rate: f64) -> Self {
        Self {
            rate,
            min_cutoff: 1.0,
            beta: 0.007,
            d_cutoff: 1.0,
            x_prev: [0.0; 52],
            dx_prev: [0.0; 52],
            initialized: false,
        }
    }

    pub fn update(&mut self, x: &[f64; 52]) -> [f64; 52] {
        if !self.initialized {
            self.x_prev = *x;
            self.initialized = true;
            return *x;
        }
        let t_e = 1.0 / self.rate;
        let a_d = smoothing_factor(t_e, self.d_cutoff);
        let mut dx = [0.0f64; 52];
        let mut dx_hat = [0.0f64; 52];
        let mut cutoff = [0.0f64; 52];
        let mut alpha = [0.0f64; 52];
        let mut x_hat = [0.0f64; 52];
        for index in 0..52 {
            dx[index] = (x[index] - self.x_prev[index]) / t_e;
            dx_hat[index] = a_d * dx[index] + (1.0 - a_d) * self.dx_prev[index];
            cutoff[index] = self.min_cutoff + self.beta * dx_hat[index].abs();
            alpha[index] = smoothing_factor(t_e, cutoff[index]);
            x_hat[index] = alpha[index] * x[index] + (1.0 - alpha[index]) * self.x_prev[index];
        }
        self.x_prev = x_hat;
        self.dx_prev = dx_hat;
        x_hat
    }
}

fn smoothing_factor(t_e: f64, cutoff: f64) -> f64 {
    let r = 2.0 * std::f64::consts::PI * cutoff * t_e;
    r / (r + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_is_returned_unchanged() {
        let mut filter = OneEuroFilter::new(25.0);
        let input = [0.5; 52];
        assert_eq!(filter.update(&input), input);
    }

    #[test]
    fn constant_signal_is_stable() {
        let mut filter = OneEuroFilter::new(25.0);
        let input = [0.5; 52];
        filter.update(&input);
        for _ in 0..10 {
            let output = filter.update(&input);
            for value in output {
                assert!((value - 0.5).abs() < 1e-12);
            }
        }
    }
}
