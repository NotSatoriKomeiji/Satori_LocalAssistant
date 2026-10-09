//! Deterministic local learning. Prediction is never an authorization.
use crate::model::{Context, Prediction, Sample};
use std::collections::HashSet;

/// Missing features are omitted and remaining weights renormalized.
/// An application/device mismatch is handled as an abstention by `predict`.
pub fn similarity(a: &Context, b: &Context) -> f64 {
    let distance = a.hour.abs_diff(b.hour).min(24 - a.hour.abs_diff(b.hour));
    let mut total = 0.60; // app 0.30 + device 0.20 + time 0.10
    let mut score = 0.30 * f64::from(a.app_id == b.app_id)
        + 0.20 * f64::from(a.device == b.device)
        + 0.10 * (1.0 - f64::from(distance) / 12.0);
    for (x, y, weight) in [(&a.activity, &b.activity, 0.25), (&a.power, &b.power, 0.05)] {
        if let (Some(x), Some(y)) = (x, y) {
            total += weight;
            score += weight * f64::from(x == y);
        }
    }
    if !a.recent.is_empty() && !b.recent.is_empty() {
        let x: HashSet<_> = a.recent.iter().collect();
        let y: HashSet<_> = b.recent.iter().collect();
        score += 0.10 * x.intersection(&y).count() as f64 / x.union(&y).count() as f64;
        total += 0.10;
    }
    (score / total).clamp(0.0, 1.0)
}

pub fn predict(
    context: &Context,
    samples: &[Sample],
    alpha: f64,
    beta: f64,
    now: i64,
) -> Option<Prediction> {
    let mut rows: Vec<_> = samples
        .iter()
        .filter(|s| {
            s.context.app_id == context.app_id
                && s.context.device == context.device
                && s.context.session != context.session
                && crate::model::valid_volume(s.volume)
                && s.at <= now
                && now - s.at <= 90 * 86400
        })
        .map(|s| (s, similarity(context, &s.context)))
        .filter(|(_, similarity)| *similarity >= 0.80)
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.at.cmp(&a.0.at)));
    rows.truncate(20);
    // Cold start never prompts based on one incidental setting.
    if rows.len() < 3 {
        return None;
    }
    let mut values: Vec<_> = rows.iter().map(|(s, _)| s.volume).collect();
    values.sort_by(f64::total_cmp);
    let median = if values.len() % 2 == 0 {
        (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
    } else {
        values[values.len() / 2]
    };
    rows.retain(|(s, _)| (s.volume - median).abs() <= 0.15);
    let sessions: HashSet<_> = rows.iter().map(|(s, _)| &s.context.session).collect();
    if rows.len() < 3 || sessions.len() < 3 {
        return None;
    }
    rows.sort_by_key(|(s, _)| s.at);
    let mut weight_sum = 0.0;
    let mut weighted = 0.0;
    let mut match_score = 0.0;
    let mut ema = rows[0].0.volume;
    for (s, sim) in &rows {
        let decay = (-(now - s.at).max(0) as f64 / (30.0 * 86400.0)).exp();
        let weight = decay * sim;
        weight_sum += weight;
        weighted += s.volume * weight;
        match_score += sim * weight;
        ema = 0.10 * s.volume + 0.90 * ema;
    }
    let target = 0.75 * weighted / weight_sum + 0.25 * ema;
    let similarity = match_score / weight_sum;
    let deviation = rows
        .iter()
        .map(|(s, _)| (s.volume - target).abs())
        .sum::<f64>()
        / rows.len() as f64;
    let consistency = (1.0 - deviation / 0.15).clamp(0.0, 1.0);
    let support = (rows.len() as f64 / 5.0).min(1.0);
    let feedback_mean = alpha / (alpha + beta);
    // Beta(1,1) starts neutral. Feedback changes the prediction slightly, with a cap.
    let feedback_factor = 0.95 + 0.05 * (2.0 * feedback_mean - 1.0);
    let confidence =
        (similarity * consistency * (0.85 + 0.15 * support) * feedback_factor).clamp(0.0, 1.0);
    Some(Prediction {
        target,
        confidence,
        similarity,
        feedback_mean,
        samples: rows.len(),
        sessions: sessions.len(),
        reason: format!(
            "同一软件、同一音频设备，{} 个独立场景样本；相似度 {:.0}%，偏好稳定度 {:.0}%",
            rows.len(),
            similarity * 100.0,
            consistency * 100.0
        ),
    })
}
