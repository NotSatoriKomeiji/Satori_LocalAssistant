//! In-process, compile-time extension contract. This is not a security sandbox.
//! Providers return rank hints only; Engine owns all permissions and execution.
use crate::{
    model::{Context, Usage},
    Result,
};
use serde::{Deserialize, Serialize};
pub const API_VERSION: u32 = 1;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub autonomous: bool,
    pub authorization: String,
}
pub fn capabilities(available: bool) -> Vec<Capability> {
    vec![
        Capability {
            id: "volume".into(),
            name: "主音量".into(),
            available,
            autonomous: true,
            authorization: "普通场景自动微调；暂停、敏感场景和手动覆盖优先".into(),
        },
        Capability {
            id: "brightness".into(),
            name: "外接屏亮度".into(),
            available,
            autonomous: true,
            authorization: "桌面调节层使用DDC/CI；实际支持以设备探测为准".into(),
        },
        Capability {
            id: "open_website".into(),
            name: "打开站点首页".into(),
            available,
            autonomous: false,
            authorization: "仅明确点击，通过默认浏览器打开HTTPS首页".into(),
        },
        Capability {
            id: "open_app".into(),
            name: "打开已选应用".into(),
            available,
            autonomous: false,
            authorization: "仅明确点击；不能自动启动".into(),
        },
    ]
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub api_version: u32,
    pub scope: String,
}
#[derive(Debug, Clone)]
pub struct RankHint {
    pub app_id: String,
    pub score: f64,
    pub independent_days: usize,
}
pub trait SuggestionProvider: Send {
    fn info(&self) -> ExtensionInfo;
    fn suggest(&self, context: &Context, usage: &[Usage], at: i64) -> Vec<RankHint>;
}
#[derive(Default)]
pub struct Registry {
    providers: Vec<Box<dyn SuggestionProvider>>,
}
impl Registry {
    pub fn standard() -> Self {
        let mut registry = Self::default();
        registry
            .register(Box::new(crate::associations::Associations))
            .expect("built-in extension");
        registry
    }
    pub fn register(&mut self, provider: Box<dyn SuggestionProvider>) -> Result<()> {
        let info = provider.info();
        if info.api_version != API_VERSION
            || info.scope != "usage_metadata"
            || info.id.is_empty()
            || !info
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || self.providers.iter().any(|p| p.info().id == info.id)
        {
            return Err(crate::rule("扩展版本、标识或范围无效"));
        }
        self.providers.push(provider);
        Ok(())
    }
    pub fn info(&self) -> Vec<ExtensionInfo> {
        self.providers.iter().map(|p| p.info()).collect()
    }
    pub fn suggest(&self, context: &Context, usage: &[Usage], at: i64) -> Vec<RankHint> {
        self.providers
            .iter()
            .flat_map(|p| p.suggest(context, usage, at))
            .filter(|h| {
                h.independent_days >= 3 && h.score.is_finite() && (0.0..=1.0).contains(&h.score)
            })
            .take(128)
            .collect()
    }
}
