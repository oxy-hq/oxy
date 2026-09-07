//! A deterministic generator the simulation owns outright.
//!
//! Deliberately **not** `rand`. A `seed:` in a `.simulation.yml` has to mean the
//! same world in a year's time: recorded runs are evidence, and the whole point
//! of the exercise is comparing an estimate against a truth we wrote down. `rand`
//! makes no guarantee that a generator's output stream is stable across releases,
//! so a routine dependency bump would silently reshape every declared world —
//! with nothing failing, because a different world is still a perfectly valid
//! world. SplitMix64 is ten lines and pins it for good.

/// SplitMix64. Chosen for being short enough to audit at a glance while still
/// passing the usual statistical batteries — this draws noise, it does not
/// protect anything.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Derive an independent stream from a base seed and a label.
    ///
    /// Every mechanism and every entity draws from its own stream, so **adding**
    /// one cannot shift the draws of the others. `gen_restaurant_data.py` learned
    /// this the expensive way: introducing a single new driver pair moved an
    /// unrelated fitted coefficient from 5.783 to 5.751, and with it every figure
    /// in the view, the docstrings and `internal-docs/`. A shared stream makes
    /// every world fragile to edits that have nothing to do with it.
    pub fn stream(seed: u64, label: &str) -> Self {
        // FNV-1a, so the label→stream mapping is fixed here rather than inherited
        // from whatever `DefaultHasher` happens to be this release.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in label.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self::new(seed ^ hash)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform on `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        // 53 bits is exactly what an f64 mantissa holds, so this can never round
        // up to 1.0 — which `ln` in `normal()` below would not survive.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal, via Box–Muller.
    pub fn normal(&mut self) -> f64 {
        // `1.0 - uniform()` lands in (0, 1], keeping `ln` defined. Taking
        // `uniform()` directly would eventually draw an exact 0.0 and produce
        // an infinity — rare enough to survive every test run and still ruin a
        // long sweep.
        let u1 = 1.0 - self.uniform();
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Normal with the given mean and standard deviation.
    pub fn gauss(&mut self, mean: f64, sd: f64) -> f64 {
        mean + sd * self.normal()
    }

    /// Lognormal with unit median and the given log-space spread. Used for entity
    /// scale, where the point is a right-skewed spread of sizes around a typical
    /// one rather than a symmetric band.
    pub fn lognormal(&mut self, sigma: f64) -> f64 {
        (sigma * self.normal()).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_the_same_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn labelled_streams_are_independent_of_one_another() {
        // The property that matters is not that the streams differ — any hash
        // gives that — but that one stream's draws do not depend on whether
        // another was ever constructed.
        let mut alpha = Rng::stream(7, "marketing");
        let first: Vec<u64> = (0..8).map(|_| alpha.next_u64()).collect();

        let mut _unrelated = Rng::stream(7, "delivery");
        for _ in 0..100 {
            _unrelated.next_u64();
        }

        let mut alpha_again = Rng::stream(7, "marketing");
        let second: Vec<u64> = (0..8).map(|_| alpha_again.next_u64()).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn uniform_stays_in_range() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            let u = rng.uniform();
            assert!((0.0..1.0).contains(&u), "uniform out of range: {u}");
        }
    }

    #[test]
    fn normal_is_roughly_standard() {
        let mut rng = Rng::new(42);
        let n = 100_000;
        let draws: Vec<f64> = (0..n).map(|_| rng.normal()).collect();
        let mean = draws.iter().sum::<f64>() / n as f64;
        let var = draws.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;

        // Loose bounds: this asserts the transform is not wrong, not that the
        // generator is good. A Box–Muller that dropped the sqrt or used the
        // wrong constant misses these by a mile.
        assert!(mean.abs() < 0.02, "mean drifted: {mean}");
        assert!((var - 1.0).abs() < 0.03, "variance drifted: {var}");
        assert!(draws.iter().all(|x| x.is_finite()), "non-finite draw");
    }
}
