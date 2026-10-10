//! Brightness learning uses independent manual choices, never its own output.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub app: String,
    pub device: String,
    pub period: u8,
    pub value: u8,
    pub at: i64,
    pub session: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Preferences {
    pub choices: Vec<Choice>,
}
impl Preferences {
    pub fn record(&mut self, c: Choice) {
        if c.value > 100 {
            return;
        }
        self.choices.retain(|v| {
            !(v.device == c.device && v.session == c.session) && c.at - v.at < 90 * 86400
        });
        self.choices.push(c);
        if self.choices.len() > 500 {
            self.choices.drain(..self.choices.len() - 500);
        }
    }
    pub fn target(
        &self,
        app: &str,
        device: &str,
        hour: u8,
        current: u8,
        session: &str,
        now: i64,
    ) -> Option<u8> {
        let rows: Vec<_> = self
            .choices
            .iter()
            .filter(|c| {
                c.app == app
                    && c.device == device
                    && c.period == hour / 6
                    && c.session != session
                    && c.at <= now
                    && now - c.at < 90 * 86400
            })
            .collect();
        if rows.len() < 3 {
            return None;
        }
        let mut values: Vec<_> = rows.iter().map(|c| c.value).collect();
        values.sort();
        let median = values[values.len() / 2];
        if values.iter().filter(|v| v.abs_diff(median) <= 10).count() * 2 < values.len() + 1 {
            return None;
        }
        let delta = (i16::from(median) - i16::from(current)).clamp(-5, 5);
        if delta.abs() < 2 {
            return None;
        }
        let value = i16::from(current) + delta;
        (10..=100).contains(&value).then_some(value as u8)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_scenes_bounded_and_device_scoped() {
        let mut p = Preferences::default();
        for i in 0..3 {
            p.record(Choice {
                app: "edit".into(),
                device: "screen".into(),
                period: 2,
                value: 70,
                at: i,
                session: format!("s{i}"),
            });
        }
        assert_eq!(p.target("edit", "screen", 12, 30, "new", 5), Some(35));
        assert_eq!(p.target("edit", "other", 12, 30, "new", 5), None);
        assert_eq!(p.target("edit", "screen", 12, 30, "s1", 5), None);
        p.record(Choice {
            app: "edit".into(),
            device: "screen".into(),
            period: 2,
            value: 20,
            at: 6,
            session: "s1".into(),
        });
        assert_eq!(p.choices.len(), 3);
    }
}
