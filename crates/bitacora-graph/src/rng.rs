//! Tiny deterministic LCG (same family d3 uses for jiggle); no external dependency.

#[derive(Debug, Clone)]
pub(crate) struct Lcg(u32);

impl Lcg {
    pub(crate) fn new(seed: u64) -> Self {
        Self(((seed ^ (seed >> 32)) as u32) | 1)
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Tiny non-zero offset used to separate coincident points.
    pub(crate) fn jiggle(&mut self) -> f32 {
        (self.next_f32() - 0.5) * 1e-6
    }
}
