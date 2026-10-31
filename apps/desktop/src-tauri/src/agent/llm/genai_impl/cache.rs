//! Where one request asks its provider to cache the prompt. Each provider caches differently,
//! and the wrong ask is not a no-op: on OpenAI's GPT-5.6+ a breakpoint switches genai to
//! explicit mode, which caches nothing once the last message is a tool result. Why each
//! placement, with evidence: `../DETAILS.md` § Prompt caching.

use genai::adapter::AdapterKind;
use genai::chat::{CacheControl, ChatOptions};

/// How a request asks for caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CachePlan {
    /// Mark the last message, and on Anthropic the system string too. Claude caches only what
    /// a request marks; every other provider here caches on its own and ignores the marks.
    Breakpoints,
    /// OpenAI's own GPT-5.6 and later: no breakpoints, and the intent said at the request level,
    /// so genai sends nothing and OpenAI's automatic caching covers the growing prefix, tool
    /// results included. A breakpoint can sit only on a text block there, and genai's explicit
    /// mode with none caches nothing at all.
    Automatic,
    /// OpenRouter routing to Claude: one top-level `cache_control`, OpenRouter's automatic mode
    /// for Claude, since genai's OpenAI adapter drops per-message marks.
    TopLevel,
}

impl CachePlan {
    /// The plan for a request to `model` on `adapter`. `top_level` is the backend's
    /// `wants_top_level_cache_marker` (OpenRouter to Claude).
    pub(super) fn for_request(adapter: AdapterKind, model: &str, top_level: bool) -> Self {
        if top_level {
            Self::TopLevel
        } else if matches!(adapter, AdapterKind::OpenAI | AdapterKind::OpenAIResp) && caches_explicitly(model) {
            Self::Automatic
        } else {
            Self::Breakpoints
        }
    }

    /// Whether the request's messages carry breakpoints.
    pub(super) fn marks_messages(self) -> bool {
        self == Self::Breakpoints
    }

    /// `options` with the request-level part of the plan.
    pub(super) fn apply(self, options: ChatOptions) -> ChatOptions {
        match self {
            Self::Breakpoints => options,
            // Request-level intent with no breakpoints is what makes genai leave OpenAI's
            // automatic mode on; on these models it sends no field for it.
            Self::Automatic => options.with_cache_control(CacheControl::Ephemeral),
            Self::TopLevel => options.with_extra_body(serde_json::json!({ "cache_control": { "type": "ephemeral" } })),
        }
    }
}

/// OpenAI's own `gpt-N.M…` ids from GPT-5.6 on, the models genai 0.7.1 switches to explicit
/// caching (it mirrors genai's `OpenAIModel::requires_explicit_cache`, which isn't public). An
/// id carrying a maker prefix (`openai/gpt-6.1-sol` on OpenRouter) is some other host's, so
/// it stays on [`CachePlan::Breakpoints`].
fn caches_explicitly(model: &str) -> bool {
    let Some(rest) = model.strip_prefix("gpt-") else {
        return false;
    };
    let version = rest.split('-').next().unwrap_or_default();
    let mut parts = version.split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    match (major, minor) {
        (Some(major), Some(minor)) => major > 5 || (major == 5 && minor >= 6),
        (Some(major), None) => major > 5,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_from_gpt_5_6_on_caches_automatically() {
        for model in ["gpt-5.6-terra", "gpt-5.6-sol", "gpt-6.1-sol", "gpt-6-luna", "gpt-5.10"] {
            assert_eq!(
                CachePlan::for_request(AdapterKind::OpenAIResp, model, false),
                CachePlan::Automatic,
                "{model}"
            );
        }
        assert_eq!(
            CachePlan::for_request(AdapterKind::OpenAI, "gpt-6.1-terra", false),
            CachePlan::Automatic
        );
    }

    #[test]
    fn everything_else_marks_breakpoints_or_rides_the_top_level() {
        for (adapter, model) in [
            (AdapterKind::OpenAIResp, "gpt-5.5"),
            (AdapterKind::OpenAIResp, "gpt-5"),
            (AdapterKind::OpenAI, "gpt-4.1-mini"),
            (AdapterKind::OpenAI, "openai/gpt-6.1-sol"),
            (AdapterKind::Anthropic, "claude-sonnet-5-5"),
            (AdapterKind::Gemini, "gemini-3.6-flash"),
        ] {
            assert_eq!(
                CachePlan::for_request(adapter, model, false),
                CachePlan::Breakpoints,
                "{model}"
            );
        }
        assert_eq!(
            CachePlan::for_request(AdapterKind::OpenAI, "anthropic/claude-sonnet-5.5", true),
            CachePlan::TopLevel
        );
    }

    #[test]
    fn only_breakpoints_mark_messages() {
        assert!(CachePlan::Breakpoints.marks_messages());
        assert!(!CachePlan::Automatic.marks_messages());
        assert!(!CachePlan::TopLevel.marks_messages());
        let automatic = CachePlan::Automatic.apply(ChatOptions::default());
        assert!(automatic.cache_control.is_some() && automatic.extra_body.is_none());
    }
}
