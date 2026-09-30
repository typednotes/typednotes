//! Provider contracts. API roots and protocols were checked against the
//! providers' docs and pi-ai's public provider definitions on 2026-09-30.
use crate::Provider;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiApi {
    Chat,
    Messages,
    Responses,
    Gemini,
    Pi,
    Classifier,
}
impl AiApi {
    pub fn id(self) -> &'static str {
        match self {
            Self::Chat => "openai",
            Self::Messages => "anthropic",
            Self::Responses => "responses",
            Self::Gemini => "gemini",
            Self::Pi => "pi",
            Self::Classifier => "classifier",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiProviderInfo {
    pub provider: Provider,
    pub id: &'static str,
    pub name: &'static str,
    pub env: &'static str,
    pub base_url: Option<&'static str>,
    pub api: AiApi,
    pub default_model: Option<&'static str>,
    pub pricing_id: Option<&'static str>,
    pub subscription: bool,
}

macro_rules! ai {
    ($p:ident,$id:literal,$name:literal,$env:literal,$base:expr,$api:ident,$model:expr,$pricing:expr,$plan:expr) => {
        AiProviderInfo {
            provider: Provider::$p,
            id: $id,
            name: $name,
            env: $env,
            base_url: $base,
            api: AiApi::$api,
            default_model: $model,
            pricing_id: $pricing,
            subscription: $plan,
        }
    };
}

pub const AI_PROVIDERS: &[AiProviderInfo] = &[
    ai!(
        AntLing,
        "ant-ling",
        "Ant Ling",
        "ANT_LING_API_KEY",
        Some("https://api.ant-ling.com/v1"),
        Chat,
        None,
        None,
        false
    ),
    ai!(
        Anthropic,
        "anthropic",
        "Anthropic",
        "ANTHROPIC_API_KEY",
        Some("https://api.anthropic.com/v1"),
        Messages,
        Some("claude-sonnet-4-5"),
        Some("anthropic"),
        false
    ),
    ai!(
        Baseten,
        "baseten",
        "Baseten",
        "BASETEN_API_KEY",
        Some("https://inference.baseten.co/v1"),
        Chat,
        None,
        Some("baseten"),
        false
    ),
    ai!(
        Cerebras,
        "cerebras",
        "Cerebras",
        "CEREBRAS_API_KEY",
        Some("https://api.cerebras.ai/v1"),
        Chat,
        Some("gpt-oss-120b"),
        Some("cerebras"),
        false
    ),
    ai!(
        Deepseek,
        "deepseek",
        "DeepSeek",
        "DEEPSEEK_API_KEY",
        Some("https://api.deepseek.com/v1"),
        Chat,
        Some("deepseek-chat"),
        Some("deepseek"),
        false
    ),
    ai!(
        Fireworks,
        "fireworks",
        "Fireworks",
        "FIREWORKS_API_KEY",
        Some("https://api.fireworks.ai/inference/v1"),
        Chat,
        None,
        Some("fireworks-ai"),
        false
    ),
    ai!(
        GithubCopilot,
        "github-copilot",
        "GitHub Copilot",
        "COPILOT_GITHUB_TOKEN",
        Some("https://api.individual.githubcopilot.com"),
        Chat,
        Some("gpt-4.1"),
        Some("github-copilot"),
        true
    ),
    ai!(
        Gemini,
        "gemini",
        "Google Gemini",
        "GEMINI_API_KEY",
        Some("https://generativelanguage.googleapis.com/v1beta"),
        Gemini,
        Some("gemini-2.5-flash"),
        Some("google"),
        false
    ),
    ai!(
        Groq,
        "groq",
        "Groq",
        "GROQ_API_KEY",
        Some("https://api.groq.com/openai/v1"),
        Chat,
        Some("llama-3.3-70b-versatile"),
        Some("groq"),
        false
    ),
    ai!(
        HuggingFace,
        "huggingface",
        "Hugging Face",
        "HF_TOKEN",
        Some("https://router.huggingface.co/v1"),
        Chat,
        None,
        Some("huggingface"),
        false
    ),
    ai!(
        KimiCoding,
        "kimi-coding",
        "Kimi For Coding",
        "KIMI_API_KEY",
        Some("https://api.kimi.com/coding/v1"),
        Messages,
        Some("kimi-for-coding"),
        Some("kimi-code-plan-cn"),
        true
    ),
    ai!(
        Meta,
        "meta",
        "Meta",
        "META_API_KEY",
        Some("https://api.meta.ai/v1"),
        Responses,
        None,
        Some("meta"),
        false
    ),
    ai!(
        Minimax,
        "minimax",
        "MiniMax",
        "MINIMAX_API_KEY",
        Some("https://api.minimax.io/anthropic/v1"),
        Messages,
        Some("MiniMax-M2.7"),
        Some("minimax"),
        false
    ),
    ai!(
        MinimaxCn,
        "minimax-cn",
        "MiniMax (China)",
        "MINIMAX_CN_API_KEY",
        Some("https://api.minimaxi.com/anthropic/v1"),
        Messages,
        Some("MiniMax-M2.7"),
        Some("minimax-cn"),
        false
    ),
    ai!(
        Mistral,
        "mistral",
        "Mistral",
        "MISTRAL_API_KEY",
        Some("https://api.mistral.ai/v1"),
        Chat,
        Some("mistral-large-latest"),
        Some("mistral"),
        false
    ),
    ai!(
        Moonshot,
        "moonshotai",
        "Moonshot AI (Global)",
        "MOONSHOT_API_KEY",
        Some("https://api.moonshot.ai/v1"),
        Chat,
        Some("kimi-k2.5"),
        Some("moonshotai"),
        false
    ),
    ai!(
        MoonshotCn,
        "moonshotai-cn",
        "Moonshot AI (China)",
        "MOONSHOT_API_KEY",
        Some("https://api.moonshot.cn/v1"),
        Chat,
        Some("kimi-k2.5"),
        Some("moonshotai-cn"),
        false
    ),
    ai!(
        Nvidia,
        "nvidia",
        "NVIDIA NIM",
        "NVIDIA_API_KEY",
        Some("https://integrate.api.nvidia.com/v1"),
        Chat,
        None,
        Some("nvidia"),
        false
    ),
    ai!(
        Openai,
        "openai",
        "OpenAI",
        "OPENAI_API_KEY",
        Some("https://api.openai.com/v1"),
        Chat,
        Some("gpt-4.1"),
        Some("openai"),
        false
    ),
    ai!(
        OpenaiCompatible,
        "openai-compatible",
        "OpenAI-compatible",
        "OPENAI_API_KEY",
        None,
        Chat,
        None,
        None,
        false
    ),
    ai!(
        OpencodeGo,
        "opencode-go",
        "OpenCode Go",
        "OPENCODE_API_KEY",
        Some("https://opencode.ai/zen/go/v1"),
        Chat,
        None,
        Some("opencode-go"),
        true
    ),
    ai!(
        OpencodeZen,
        "opencode",
        "OpenCode Zen",
        "OPENCODE_API_KEY",
        Some("https://opencode.ai/zen/v1"),
        Chat,
        None,
        Some("opencode"),
        false
    ),
    ai!(
        Openrouter,
        "openrouter",
        "OpenRouter",
        "OPENROUTER_API_KEY",
        Some("https://openrouter.ai/api/v1"),
        Chat,
        Some("anthropic/claude-sonnet-4.5"),
        Some("openrouter"),
        false
    ),
    ai!(
        QwenTokenPlan,
        "qwen-token-plan",
        "Qwen Token Plan",
        "QWEN_TOKEN_PLAN_API_KEY",
        Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
        Chat,
        None,
        Some("alibaba-token-plan"),
        true
    ),
    ai!(
        QwenTokenPlanCn,
        "qwen-token-plan-cn",
        "Qwen Token Plan (China)",
        "QWEN_TOKEN_PLAN_CN_API_KEY",
        Some("https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"),
        Chat,
        None,
        Some("alibaba-token-plan-cn"),
        true
    ),
    ai!(
        QwenTokenPlanIndividual,
        "qwen-token-plan-individual",
        "Qwen Token Plan Individual",
        "QWEN_TOKEN_PLAN_API_KEY",
        Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
        Chat,
        None,
        Some("alibaba-token-plan"),
        true
    ),
    ai!(
        Radius,
        "radius",
        "Radius",
        "RADIUS_API_KEY",
        Some("https://radius.pi.dev/v1"),
        Pi,
        None,
        None,
        false
    ),
    ai!(
        Scaleway,
        "scaleway",
        "Scaleway",
        "SCW_SECRET_KEY",
        Some("https://api.scaleway.ai/v1"),
        Chat,
        None,
        Some("scaleway"),
        false
    ),
    ai!(
        Together,
        "together",
        "Together AI",
        "TOGETHER_API_KEY",
        Some("https://api.together.xyz/v1"),
        Chat,
        None,
        Some("togetherai"),
        false
    ),
    ai!(
        TypeSafe,
        "typesafe",
        "TypeSafe (classifier models)",
        "TYPESAFE_API_KEY",
        Some("https://api.typesafe.ai/v1"),
        Classifier,
        Some("jev-latest"),
        None,
        false
    ),
    ai!(
        VercelAiGateway,
        "vercel-ai-gateway",
        "Vercel AI Gateway",
        "AI_GATEWAY_API_KEY",
        Some("https://ai-gateway.vercel.sh/v1"),
        Chat,
        None,
        Some("vercel"),
        false
    ),
    ai!(
        Xiaomi,
        "xiaomi",
        "Xiaomi MiMo",
        "XIAOMI_API_KEY",
        Some("https://api.xiaomimimo.com/v1"),
        Chat,
        None,
        Some("xiaomi"),
        false
    ),
    ai!(
        XiaomiTokenPlanAms,
        "xiaomi-token-plan-ams",
        "Xiaomi MiMo Token Plan (Amsterdam)",
        "XIAOMI_TOKEN_PLAN_AMS_API_KEY",
        Some("https://token-plan-ams.xiaomimimo.com/v1"),
        Chat,
        None,
        Some("xiaomi-token-plan-ams"),
        true
    ),
    ai!(
        XiaomiTokenPlanCn,
        "xiaomi-token-plan-cn",
        "Xiaomi MiMo Token Plan (China)",
        "XIAOMI_TOKEN_PLAN_CN_API_KEY",
        Some("https://token-plan-cn.xiaomimimo.com/v1"),
        Chat,
        None,
        Some("xiaomi-token-plan-cn"),
        true
    ),
    ai!(
        XiaomiTokenPlanSgp,
        "xiaomi-token-plan-sgp",
        "Xiaomi MiMo Token Plan (Singapore)",
        "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
        Some("https://token-plan-sgp.xiaomimimo.com/v1"),
        Chat,
        None,
        Some("xiaomi-token-plan-sgp"),
        true
    ),
    ai!(
        ZaiCodingCn,
        "zai-coding-cn",
        "ZAI Coding Plan (China)",
        "ZAI_CODING_CN_API_KEY",
        Some("https://open.bigmodel.cn/api/coding/paas/v4"),
        Chat,
        None,
        Some("zhipuai-coding-plan"),
        true
    ),
    ai!(
        Zai,
        "zai",
        "ZAI Coding Plan (Global)",
        "ZAI_API_KEY",
        Some("https://api.z.ai/api/coding/paas/v4"),
        Chat,
        None,
        Some("zai-coding-plan"),
        true
    ),
    ai!(
        Xai,
        "xai",
        "xAI",
        "XAI_API_KEY",
        Some("https://api.x.ai/v1"),
        Responses,
        Some("grok-4"),
        Some("xai"),
        false
    ),
];

impl Provider {
    pub fn ai_info(self) -> Option<&'static AiProviderInfo> {
        AI_PROVIDERS.iter().find(|p| p.provider == self)
    }
    pub fn can_generate(self) -> bool {
        self.ai_info().is_some_and(|p| p.api != AiApi::Classifier)
    }
    pub fn ai_api(self, model: &str) -> Option<AiApi> {
        let info = self.ai_info()?;
        if matches!(self, Self::OpencodeZen | Self::OpencodeGo) {
            // Go and Zen expose different native APIs for the same model.
            if self == Self::OpencodeZen && model.starts_with("jev-") {
                return Some(AiApi::Classifier);
            }
            if self == Self::OpencodeZen && model == "qwen3.8-max" {
                return Some(AiApi::Chat);
            }
            if model.starts_with("claude-")
                || model.starts_with("qwen")
                || (self == Self::OpencodeGo && model.starts_with("minimax-"))
            {
                return Some(AiApi::Messages);
            }
            if self == Self::OpencodeZen && model.starts_with("gemini-") {
                return Some(AiApi::Gemini);
            }
            if uses_openai_responses(model)
                || model.starts_with("grok-")
                || model.starts_with("muse-spark-")
            {
                return Some(AiApi::Responses);
            }
        }
        if self == Self::GithubCopilot {
            if model.starts_with("claude-") {
                return Some(AiApi::Messages);
            }
            // Copilot's older GPT models and GPT-5 Mini use Chat Completions.
            if model.starts_with("gpt-")
                && uses_openai_responses(model)
                && !model.starts_with("gpt-5-mini")
            {
                return Some(AiApi::Responses);
            }
        }
        if self == Self::Openai && uses_openai_responses(model) {
            return Some(AiApi::Responses);
        }
        Some(info.api)
    }
}

fn uses_openai_responses(model: &str) -> bool {
    model
        .strip_prefix("gpt-")
        .and_then(|name| name.split(['.', '-']).next())
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 5)
        || ["o1", "o3", "o4"].iter().any(|prefix| model.starts_with(prefix))
}

/// USD per million tokens, as published by the linked catalog. Missing means
/// unknown, not free. Subscription/plan charges are separate from token rates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TokenPricing {
    pub provider: Provider,
    pub model: String,
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
    pub source: Option<String>,
    pub subscription: bool,
    pub note: String,
}

impl TokenPricing {
    pub fn estimate(
        &self,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
    ) -> Option<f64> {
        if self.subscription {
            return None;
        }
        let rate = |n: u64, price: Option<f64>| {
            if n == 0 {
                Some(0.0)
            } else {
                price.map(|p| n as f64 * p / 1_000_000.0)
            }
        };
        let total = rate(input, self.input)?
            + rate(output, self.output)?
            + rate(cache_read, self.cache_read)?
            + rate(cache_write, self.cache_write)?;
        total.is_finite().then_some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_requested_key_has_a_named_connector() {
        for key in [
            "ANTHROPIC_API_KEY",
            "ANT_LING_API_KEY",
            "OPENAI_API_KEY",
            "DEEPSEEK_API_KEY",
            "NVIDIA_API_KEY",
            "GEMINI_API_KEY",
            "COPILOT_GITHUB_TOKEN",
            "MISTRAL_API_KEY",
            "GROQ_API_KEY",
            "CEREBRAS_API_KEY",
            "XAI_API_KEY",
            "OPENROUTER_API_KEY",
            "AI_GATEWAY_API_KEY",
            "ZAI_API_KEY",
            "ZAI_CODING_CN_API_KEY",
            "OPENCODE_API_KEY",
            "RADIUS_API_KEY",
            "TYPESAFE_API_KEY",
            "HF_TOKEN",
            "FIREWORKS_API_KEY",
            "TOGETHER_API_KEY",
            "BASETEN_API_KEY",
            "KIMI_API_KEY",
            "META_API_KEY",
            "MINIMAX_API_KEY",
            "MINIMAX_CN_API_KEY",
            "MOONSHOT_API_KEY",
            "QWEN_TOKEN_PLAN_API_KEY",
            "QWEN_TOKEN_PLAN_CN_API_KEY",
            "XIAOMI_API_KEY",
            "XIAOMI_TOKEN_PLAN_CN_API_KEY",
            "XIAOMI_TOKEN_PLAN_AMS_API_KEY",
            "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
            "SCW_SECRET_KEY",
        ] {
            assert!(AI_PROVIDERS.iter().any(|p| p.env == key), "missing {key}");
        }
        assert_eq!(Provider::AI.len(), AI_PROVIDERS.len());
        assert_eq!(Provider::AI.len(), 38);
        for &p in Provider::AI {
            assert_eq!(AI_PROVIDERS.iter().filter(|i| i.provider == p).count(), 1);
            assert_eq!(Provider::from_id(p.id()), Some(p));
        }
    }
    #[test]
    fn protocols_and_regional_endpoints_are_distinct() {
        assert_eq!(
            Provider::Gemini.ai_api("gemini-2.5-flash"),
            Some(AiApi::Gemini)
        );
        assert_eq!(
            Provider::OpencodeZen.ai_api("claude-sonnet-4-5"),
            Some(AiApi::Messages)
        );
        assert_eq!(
            Provider::OpencodeZen.ai_api("gpt-5"),
            Some(AiApi::Responses)
        );
        assert_eq!(
            Provider::OpencodeZen.ai_api("gemini-2.5-flash"),
            Some(AiApi::Gemini)
        );
        assert_eq!(Provider::Radius.ai_api("m"), Some(AiApi::Pi));
        assert!(!Provider::TypeSafe.can_generate());
        for (a, b) in [
            (Provider::Moonshot, Provider::MoonshotCn),
            (Provider::Zai, Provider::ZaiCodingCn),
            (Provider::Minimax, Provider::MinimaxCn),
            (Provider::XiaomiTokenPlanCn, Provider::XiaomiTokenPlanAms),
        ] {
            assert_ne!(a.fixed_base_url(), b.fixed_base_url());
        }
    }

    #[test]
    fn gateway_model_exceptions_use_the_documented_protocol() {
        for (provider, model, api) in [
            (Provider::OpencodeZen, "qwen3.8-max", AiApi::Chat),
            (Provider::OpencodeGo, "qwen3.8-max", AiApi::Messages),
            (Provider::OpencodeGo, "minimax-m2.7", AiApi::Messages),
            (Provider::OpencodeZen, "minimax-m2.7", AiApi::Chat),
            (Provider::OpencodeZen, "muse-spark-1.3", AiApi::Responses),
            (Provider::OpencodeGo, "muse-spark-1.3-contributor", AiApi::Responses),
            (Provider::OpencodeZen, "jev-1.13", AiApi::Classifier),
            (Provider::GithubCopilot, "gpt-4.1", AiApi::Chat),
            (Provider::GithubCopilot, "gpt-4o", AiApi::Chat),
            (Provider::GithubCopilot, "gpt-5-mini", AiApi::Chat),
            (Provider::GithubCopilot, "gpt-5.4", AiApi::Responses),
            (Provider::GithubCopilot, "grok-code-fast-1", AiApi::Chat),
            (Provider::Openai, "gpt-6.1-sol", AiApi::Responses),
            (Provider::Openai, "gpt-4.1", AiApi::Chat),
        ] {
            assert_eq!(provider.ai_api(model), Some(api), "{provider:?}/{model}");
        }
    }
}
