//! Local, bounded vocabulary. No raw keystroke stream or full document history.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub uses: u32,
    pub last: i64,
    #[serde(default)]
    pub pinned: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickWords {
    pub enabled: bool,
    pub limit: usize,
    pub words: Vec<Word>,
    pub dismissed: Vec<String>,
}
impl Default for QuickWords {
    fn default() -> Self {
        Self {
            enabled: false,
            limit: 50,
            dismissed: vec![],
            words: vec![
                Word {
                    text: "收到，我会尽快处理。".into(),
                    uses: 2,
                    last: 0,
                    pinned: true,
                },
                Word {
                    text: "谢谢你的帮助！".into(),
                    uses: 2,
                    last: 0,
                    pinned: true,
                },
            ],
        }
    }
}
pub fn safe_text(text: &str) -> bool {
    let n = text.chars().count();
    let lower = text.to_lowercase();
    (2..=120).contains(&n)
        && !text
            .chars()
            .any(|c| c.is_numeric() || (c.is_control() && !c.is_whitespace()))
        && ![
            "@",
            "://",
            "password",
            "token",
            "secret",
            "验证码",
            "密码",
            "银行卡",
            "卡号",
            "身份证",
            "api_key",
            "api-key",
            "sk-",
            "bearer",
            "私钥",
        ]
        .iter()
        .any(|s| lower.contains(s))
        && !text
            .split_whitespace()
            .any(|s| s.len() > 40 && s.is_ascii())
}
impl QuickWords {
    pub fn learn(&mut self, text: &str, at: i64) -> bool {
        if !self.enabled || text.chars().count() > 512 {
            return false;
        }
        // Reject a sensitive whole field before extracting smaller words.
        if !safe_text(text) {
            return false;
        }
        let mut parts = vec![text.trim().to_string()];
        parts.extend(
            text.split(|c: char| c.is_whitespace() || "，。！？；,.!?;".contains(c))
                .map(str::trim)
                .filter(|s| safe_text(s))
                .map(str::to_string),
        );
        parts.sort();
        parts.dedup();
        let mut changed = false;
        for text in parts {
            if !safe_text(&text) || self.dismissed.contains(&text) {
                continue;
            }
            if let Some(w) = self.words.iter_mut().find(|w| w.text == text) {
                if at - w.last < 5 {
                    continue;
                }
                w.uses = w.uses.saturating_add(1);
                w.last = at;
            } else {
                self.words.push(Word {
                    text,
                    uses: 1,
                    last: at,
                    pinned: false,
                });
            }
            changed = true;
        }
        self.words
            .sort_by(|a, b| score(b, at).total_cmp(&score(a, at)));
        self.words.truncate(self.limit.clamp(1, 200) * 4);
        changed
    }
    pub fn ranked(&self, at: i64) -> Vec<Word> {
        let mut words: Vec<_> = self
            .words
            .iter()
            .filter(|w| w.pinned || w.uses >= 2)
            .cloned()
            .collect();
        words.sort_by(|a, b| score(b, at).total_cmp(&score(a, at)));
        words.truncate(self.limit);
        words
    }
    pub fn dismiss(&mut self, text: &str) {
        self.words.retain(|w| w.text != text);
        if safe_text(text) && !self.dismissed.iter().any(|w| w == text) {
            self.dismissed.push(text.into());
            if self.dismissed.len() > 200 {
                self.dismissed.remove(0);
            }
        }
    }
}
fn score(w: &Word, at: i64) -> f64 {
    f64::from(w.uses) * 2_f64.powf(-((at - w.last).max(0) as f64) / (30.0 * 86400.0))
        + if w.pinned { 2.0 } else { 0.0 }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_repeated_words_but_not_digits_or_secrets() {
        let mut q = QuickWords {
            enabled: true,
            words: vec![],
            ..Default::default()
        };
        assert!(q.learn("明天见", 100));
        assert!(q.ranked(100).is_empty());
        assert!(!q.learn("明天见", 101));
        assert!(q.learn("明天见", 110));
        assert_eq!(q.ranked(110)[0].text, "明天见");
        for t in [
            "密码是 hello",
            "我的验证码 abc",
            "hello@example.com",
            "卡号 一二三",
            "我的号码 123456",
        ] {
            assert!(!q.learn(t, 120));
        }
    }
    #[test]
    fn disabled_and_limits() {
        let mut q = QuickWords::default();
        assert!(!q.learn("明天见", 100));
        q.enabled = true;
        q.limit = 1;
        for t in ["明天见", "下午好", "再联系"] {
            q.learn(t, 100);
            q.learn(t, 110);
        }
        assert!(q.ranked(120).len() <= 1);
        assert!(q.words.len() <= 4);
        q.dismiss("明天见");
        q.learn("明天见", 200);
        q.learn("明天见", 210);
        assert!(
            !q.ranked(220).iter().any(|w| w.text == "明天见"),
            "deleted quick words stay suppressed"
        );
    }
    #[test]
    fn frequency_and_decay() {
        let q = QuickWords {
            enabled: true,
            limit: 50,
            words: vec![
                Word {
                    text: "旧词".into(),
                    uses: 10,
                    last: 0,
                    pinned: false,
                },
                Word {
                    text: "新词".into(),
                    uses: 3,
                    last: 86400 * 90,
                    pinned: false,
                },
            ],
            ..Default::default()
        };
        assert_eq!(q.ranked(86400 * 90)[0].text, "新词");
    }
}
