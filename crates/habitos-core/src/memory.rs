//! Importance is a retention score, not a permission or prediction confidence.
pub const LEARNING_RATE: f64 = 0.05;
pub fn importance(strength: f64, last_used: i64, at: i64, pinned: bool) -> f64 {
    if pinned {
        return 1.0;
    }
    let age = (at - last_used).max(0) as f64 / 86400.0;
    (strength * 2.0_f64.powf(-age / 30.0)).clamp(0.0, 1.0)
}
