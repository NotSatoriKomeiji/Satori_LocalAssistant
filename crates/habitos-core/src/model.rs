use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Off,
    Ask,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPermission {
    pub id: String,
    pub name: String,
    pub observe: bool,
    pub volume: Mode,
    pub sensitive: bool,
    pub explicit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub app_id: String,
    pub app_name: String,
    pub device: String,
    pub hour: u8,
    #[serde(default)]
    pub weekday: Option<u8>,
    #[serde(default)]
    pub local_day: Option<i64>,
    pub activity: Option<String>,
    pub power: Option<String>,
    pub recent: Vec<String>,
    pub session: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub context: Context,
    pub volume: f64,
    pub at: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub paused: bool,
    pub automatic_learning: bool,
    pub auto_adjustments: bool,
    pub time_recommendations: bool,
    pub floating_cards: bool,
    pub associations: bool,
    pub website_observation: bool,
    pub app_min_days: usize,
    pub app_threshold: f64,
    pub app_margin: f64,
    pub suggest_threshold: f64,
    pub auto_threshold: f64,
    pub min_auto_samples: usize,
    pub min_auto_sessions: usize,
    pub max_volume: f64,
    pub max_auto_delta: f64,
    pub retention_days: i64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            paused: false,
            automatic_learning: true,
            auto_adjustments: true,
            time_recommendations: true,
            floating_cards: true,
            associations: true,
            website_observation: false,
            app_min_days: 2,
            app_threshold: 0.50,
            app_margin: 0.08,
            suggest_threshold: 0.50,
            auto_threshold: 0.50,
            min_auto_samples: 3,
            min_auto_sessions: 3,
            max_volume: 0.80,
            max_auto_delta: 0.05,
            retention_days: 90,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> crate::Result<()> {
        if !self.suggest_threshold.is_finite()
            || !self.auto_threshold.is_finite()
            || !(0.50..=1.0).contains(&self.suggest_threshold)
            || !(self.suggest_threshold..=1.0).contains(&self.auto_threshold)
            || self.auto_threshold < 0.50
            || self.min_auto_samples < 3
            || self.min_auto_sessions < 3
            || self.min_auto_sessions > self.min_auto_samples
            || !self.max_volume.is_finite()
            || !(0.0..=1.0).contains(&self.max_volume)
            || !self.max_auto_delta.is_finite()
            || !(0.0..=0.20).contains(&self.max_auto_delta)
            || !(1..=365).contains(&self.retention_days)
            || !(2..=14).contains(&self.app_min_days)
            || !self.app_threshold.is_finite()
            || !(0.50..=0.95).contains(&self.app_threshold)
            || !self.app_margin.is_finite()
            || !(0.03..=0.20).contains(&self.app_margin)
        {
            return Err(crate::rule("设置超出允许范围"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub target: f64,
    pub confidence: f64,
    pub similarity: f64,
    pub feedback_mean: f64,
    pub samples: usize,
    pub sessions: usize,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub context: Context,
    pub prediction: Prediction,
    pub created: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub app_name: String,
    pub device: String,
    pub before: f64,
    pub after: f64,
    pub status: String,
    pub at: i64,
    pub undoable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub experience: crate::experience::ExperienceStatus,
    pub demo: bool,
    pub settings: Settings,
    pub current: Option<Context>,
    pub volume: Option<f64>,
    pub apps: Vec<AppPermission>,
    pub proposal: Option<Proposal>,
    pub journal: Vec<JournalEntry>,
    pub samples: usize,
    pub events: Vec<BehaviorEvent>,
    pub event_count: usize,
    pub message: String,
    pub memories: Vec<MemoryItem>,
    pub targets: Vec<LaunchTarget>,
    pub app_recommendation: Option<AppRecommendation>,
    pub quick_apps: Vec<RankedApp>,
    /// Manually saved launchers for the home grid. Independent of AI suggestions.
    #[serde(default)]
    pub home_apps: Vec<RankedApp>,
    pub capabilities: Vec<crate::extensions::Capability>,
    pub extensions: Vec<crate::extensions::ExtensionInfo>,
    pub browser_last_event: i64,
    #[serde(default)]
    pub startup: StartupStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorEvent {
    pub app_name: String,
    pub kind: String,
    pub at: i64,
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn valid_volume(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub app_id: String,
    pub name: String,
    pub pinned: bool,
    pub importance: f64,
    pub last_used: i64,
    pub days: usize,
    pub samples: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchTarget {
    pub kind: String,
    pub app_id: String,
    pub name: String,
    pub enabled: bool,
}
#[derive(Debug, Clone)]
pub struct Usage {
    pub app_id: String,
    pub local_day: i64,
    pub hour: u8,
    pub weekday: u8,
    pub previous: String,
    pub at: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRecommendation {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub confidence: f64,
    pub days: usize,
    pub reason: String,
    pub created: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupStatus {
    pub supported: bool,
    pub enabled: bool,
    pub simulated: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct UsageScore {
    pub app_id: String,
    pub overall: f64,
    pub period: f64,
    pub days: usize,
    pub period_days: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedApp {
    pub kind: String,
    pub app_id: String,
    pub name: String,
    pub score: f64,
    pub overall_score: f64,
    pub period_score: f64,
    pub association_score: f64,
    pub days: usize,
    pub period_days: usize,
    pub active: bool,
}
