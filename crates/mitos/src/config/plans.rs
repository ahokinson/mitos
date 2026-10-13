use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::domain::HarnessKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoTier {
    Go,
    Plus,
}

impl GoTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::Plus => "plus",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatGptTier {
    Plus,
    Pro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    OpenCodeGo(GoTier),
    OpenCodeZen,
    ChatGpt(ChatGptTier),
}

impl Plan {
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenCodeGo(GoTier::Go) => "opencode-go",
            Self::OpenCodeGo(GoTier::Plus) => "opencode-go-plus",
            Self::OpenCodeZen => "opencode-zen",
            Self::ChatGpt(ChatGptTier::Plus) => "chatgpt-plus",
            Self::ChatGpt(ChatGptTier::Pro) => "chatgpt-pro",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenCodeGo(GoTier::Go) => "OpenCode Go",
            Self::OpenCodeGo(GoTier::Plus) => "OpenCode Go Plus",
            Self::OpenCodeZen => "OpenCode Zen",
            Self::ChatGpt(ChatGptTier::Plus) => "ChatGPT Plus",
            Self::ChatGpt(ChatGptTier::Pro) => "ChatGPT Pro",
        }
    }

    pub fn matches(self, harness: HarnessKind, provider: Option<&str>) -> bool {
        match self {
            Self::OpenCodeGo(_) => {
                harness == HarnessKind::OpenCode && provider == Some("opencode-go")
            }
            Self::OpenCodeZen => harness == HarnessKind::OpenCode && provider == Some("opencode"),
            Self::ChatGpt(_) => {
                harness == HarnessKind::Codex
                    || (harness == HarnessKind::OpenCode && provider == Some("openai"))
            }
        }
    }

    pub fn go_tier_label(self) -> Option<&'static str> {
        match self {
            Self::OpenCodeGo(tier) => Some(tier.as_str()),
            Self::OpenCodeZen | Self::ChatGpt(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) const OPENCODE_GO: Self = Self::OpenCodeGo(GoTier::Go);

    #[cfg(test)]
    pub(crate) const OPENCODE_GO_PLUS: Self = Self::OpenCodeGo(GoTier::Plus);
}

#[derive(Default, Deserialize)]
struct PlansConfig {
    #[serde(default)]
    plans: ConfiguredPlans,
}

#[derive(Default, Deserialize)]
struct ConfiguredPlans {
    opencode_go: Option<String>,
    #[serde(default)]
    opencode_zen: bool,
    #[serde(default)]
    chatgpt: Option<String>,
}

/// The supported subscription catalog. Missing or malformed configuration
/// enables no plans rather than guessing which account a provider uses.
pub fn configured_plans(config_root: &Path) -> Vec<Plan> {
    let Ok(source) = fs::read_to_string(config_root.join("mitos.toml")) else {
        return Vec::new();
    };
    let Ok(config) = toml::from_str::<PlansConfig>(&source) else {
        return Vec::new();
    };
    let mut plans = Vec::new();
    match config.plans.opencode_go.as_deref() {
        Some("go") => plans.push(Plan::OpenCodeGo(GoTier::Go)),
        Some("plus") => plans.push(Plan::OpenCodeGo(GoTier::Plus)),
        _ => {}
    }
    if config.plans.opencode_zen {
        plans.push(Plan::OpenCodeZen);
    }
    match config.plans.chatgpt.as_deref() {
        Some("plus") => plans.push(Plan::ChatGpt(ChatGptTier::Plus)),
        Some("pro") => plans.push(Plan::ChatGpt(ChatGptTier::Pro)),
        _ => {}
    }
    plans
}

pub fn plan_for(plans: &[Plan], harness: HarnessKind, provider: Option<&str>) -> Option<Plan> {
    let mut candidates = plans
        .iter()
        .copied()
        .filter(|plan| plan.matches(harness, provider));
    let plan = candidates.next()?;
    candidates.next().is_none().then_some(plan)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::domain::id;

    fn plans_for(config: Option<&str>) -> Vec<Plan> {
        let root = std::env::temp_dir().join(format!("mitos-plans-test-{}", id()));
        fs::create_dir_all(&root).unwrap();
        if let Some(config) = config {
            fs::write(root.join("mitos.toml"), config).unwrap();
        }
        let plans = configured_plans(&root);
        fs::remove_dir_all(&root).unwrap();
        plans
    }

    #[test]
    fn only_named_supported_plans_are_enabled() {
        let plans = plans_for(Some(
            "[plans]\nopencode_go = \"plus\"\nopencode_zen = true\nchatgpt = \"plus\"\n",
        ));
        assert_eq!(
            plans,
            [
                Plan::OpenCodeGo(GoTier::Plus),
                Plan::OpenCodeZen,
                Plan::ChatGpt(ChatGptTier::Plus),
            ]
        );
        assert!(plans_for(Some("[plans]\nopencode_go = \"enterprise\"\n")).is_empty());
    }

    #[test]
    fn attribution_uses_fixed_harness_and_provider_sources() {
        let plans = [
            Plan::OpenCodeGo(GoTier::Plus),
            Plan::OpenCodeZen,
            Plan::ChatGpt(ChatGptTier::Plus),
        ];
        assert_eq!(
            plan_for(&plans, HarnessKind::OpenCode, Some("opencode-go")),
            Some(Plan::OpenCodeGo(GoTier::Plus))
        );
        assert_eq!(
            plan_for(&plans, HarnessKind::OpenCode, Some("opencode")),
            Some(Plan::OpenCodeZen)
        );
        assert_eq!(
            plan_for(&plans, HarnessKind::OpenCode, Some("openai")),
            Some(Plan::ChatGpt(ChatGptTier::Plus))
        );
        assert_eq!(
            plan_for(&plans, HarnessKind::Codex, None),
            Some(Plan::ChatGpt(ChatGptTier::Plus))
        );
        assert_eq!(plan_for(&plans, HarnessKind::Claude, None), None);
    }
}
