//! Conservative time/context matching with per-day evidence and prior shrinkage.
use crate::model::{AppRecommendation, Context, Usage};
use std::collections::HashMap;
pub fn predict(
    context: &Context,
    rows: &[Usage],
    app_id: &str,
    name: &str,
    at: i64,
    feedback: (f64, f64),
) -> Option<AppRecommendation> {
    predict_with_support(context, rows, app_id, name, at, feedback, 5)
}
pub fn predict_with_support(
    context: &Context,
    rows: &[Usage],
    app_id: &str,
    name: &str,
    at: i64,
    feedback: (f64, f64),
    min_days: usize,
) -> Option<AppRecommendation> {
    let weekday = context.weekday?;
    if context.hour > 23 || weekday > 6 {
        return None;
    }
    let mut days = HashMap::<i64, f64>::new();
    let mut last_used = 0;
    for row in rows
        .iter()
        .filter(|r| r.app_id == app_id && r.at < at && at - r.at <= 63 * 86400)
    {
        if row.hour > 23 || row.weekday > 6 {
            continue;
        }
        let d = context
            .hour
            .abs_diff(row.hour)
            .min(24 - context.hour.abs_diff(row.hour)) as f64;
        let time = (-0.5 * (d / 2.0).powi(2)).exp();
        if time < 0.7 {
            continue;
        }
        let day = if weekday == row.weekday {
            1.0
        } else if (weekday < 5) == (row.weekday < 5) {
            0.7
        } else {
            0.25
        };
        let sequence = if row.previous == context.app_id {
            1.0
        } else if context.recent.contains(&row.previous) {
            0.7
        } else {
            0.5
        };
        let weight = 0.75 * time + 0.20 * day + 0.05 * sequence;
        days.entry(row.local_day)
            .and_modify(|w| *w = w.max(weight))
            .or_insert(weight);
        last_used = last_used.max(row.at);
    }
    if days.len() < min_days.max(2) {
        return None;
    }
    // Four neutral pseudo-days prevent a small perfectly repeated sample from scoring 1.
    let evidence = (days.values().sum::<f64>() + 2.0) / (days.len() as f64 + 4.0);
    let inactive = ((at - last_used) as f64 / 86400.0 - 7.0).max(0.0);
    let freshness = 2.0_f64.powf(-inactive / 30.0);
    let ratio = feedback.0 / (feedback.0 + feedback.1);
    let confidence = evidence * freshness * (0.95 + 0.05 * (2.0 * ratio - 1.0));
    Some(AppRecommendation {
        id: uuid::Uuid::new_v4().to_string(),
        app_id: app_id.into(),
        app_name: name.into(),
        confidence,
        days: days.len(),
        created: at,
        reason: format!(
            "{} 个独立日期在相近时段使用；结合星期与近期软件，旧习惯会逐渐降权",
            days.len()
        ),
    })
}
