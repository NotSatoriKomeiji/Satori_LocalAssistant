//! Bounded, slowly updated usage accumulators; never permissions or probabilities.
pub const OVERALL_RATE: f64 = 0.01;
pub const PERIOD_RATE: f64 = 0.04;
pub fn period(hour: u8) -> u8 {
    match hour {
        0..=5 => 0,
        6..=10 => 1,
        11..=13 => 2,
        14..=17 => 3,
        18..=22 => 4,
        _ => 5,
    }
}
pub fn bounds(period: u8) -> (u8, u8) {
    [(0, 6), (6, 11), (11, 14), (14, 18), (18, 23), (23, 24)][period as usize]
}
pub fn decay(strength: f64, last_used: i64, at: i64) -> f64 {
    (strength * 2.0_f64.powf(-((at - last_used).max(0) as f64 / 86400.0) / 30.0)).clamp(0.0, 1.0)
}
pub fn combine(overall: f64, period: f64, association: f64) -> f64 {
    (0.20 * overall + 0.75 * period + 0.05 * association).clamp(0.0, 1.0)
}
