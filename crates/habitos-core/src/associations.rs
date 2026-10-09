//! A small A→B association learner, deduplicated by local date.
use crate::{
    extensions::{ExtensionInfo, RankHint, SuggestionProvider, API_VERSION},
    model::{Context, Usage},
    usage_score,
};
use std::collections::{HashMap, HashSet};
pub struct Associations;
impl SuggestionProvider for Associations {
    fn info(&self) -> ExtensionInfo {
        ExtensionInfo {
            id: "associations".into(),
            name: "应用联想".into(),
            api_version: API_VERSION,
            scope: "usage_metadata".into(),
        }
    }
    fn suggest(&self, context: &Context, usage: &[Usage], at: i64) -> Vec<RankHint> {
        let mut support = HashMap::<String, (HashSet<i64>, i64)>::new();
        for row in usage.iter().filter(|r| {
            r.at < at
                && at - r.at <= 63 * 86400
                && r.previous == context.app_id
                && r.app_id != context.app_id
                && r.hour <= 23
        }) {
            if usage_score::period(row.hour) != usage_score::period(context.hour) {
                continue;
            }
            let (days, last) = support.entry(row.app_id.clone()).or_default();
            days.insert(row.local_day);
            *last = (*last).max(row.at);
        }
        support
            .into_iter()
            .filter(|(_, (days, _))| days.len() >= 3)
            .map(|(app_id, (days, last))| RankHint {
                app_id,
                score: usage_score::decay(days.len() as f64 / (days.len() as f64 + 6.0), last, at),
                independent_days: days.len(),
            })
            .collect()
    }
}
