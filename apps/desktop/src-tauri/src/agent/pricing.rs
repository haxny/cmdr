//! A minimal per-model price table for honest cost estimation (spec §2.4, plan §5).
//!
//! Prices are USD per **million tokens**, quoted the way providers publish them, split into
//! prompt (input), completion (output), and the two prompt-cache rates: a cache READ (a prefix
//! the provider already holds, billed at a fraction of input) and a cache WRITE (input the call
//! stored for the next one, which Claude and GPT-5.6+ bill above plain input). Matched by
//! model-id prefix, longest first, because ids drift within a family (`gpt-4o-mini` must win over
//! `gpt-4o`).
//!
//! **PRICES DRIFT — verify against each provider's pricing page at release time.** A model NOT in the table is
//! UNPRICED: the meter records its tokens with cost 0 and `priced = false`, so the UI shows
//! the token counts with cost "unknown", never a silent $0. A local model is free/on-device:
//! cost 0 and `priced = true`. This honest miss-path is the point — an approximate estimate
//! for known models, and an explicit "unknown" (never a fake zero) for everything else.

use crate::agent::llm::types::{AgentUsage, ProviderTag};

/// The outcome of pricing one completed call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PricedCost {
    /// Integer micro-USD estimate (0 when local or unpriced).
    pub cost_micros: i64,
    /// False when the model wasn't in the table (an unknown cloud model): the cost is
    /// unknown, shown as such, never a silent $0.
    pub priced: bool,
}

/// One family's price, USD per million tokens. Matched by id prefix.
struct ModelPrice {
    prefix: &'static str,
    prompt_usd_per_mtok: f64,
    completion_usd_per_mtok: f64,
    /// A prompt token read back from the provider's cache.
    cache_read_usd_per_mtok: f64,
    /// A prompt token written to the cache. Plain input where the provider charges nothing extra.
    cache_write_usd_per_mtok: f64,
}

/// A Claude row. Every Claude bills a 5-minute cache write at 1.25x input (the TTL Cmdr asks
/// for) and a cache read at `read_share` of input: 0.1 for most, 0.05 for Opus 5.5 and Sonnet
/// 5.5, 0.025 for Fable 5.1 and Mythos 5.1.
const fn claude(prefix: &'static str, input: f64, output: f64, read_share: f64) -> ModelPrice {
    ModelPrice {
        prefix,
        prompt_usd_per_mtok: input,
        completion_usd_per_mtok: output,
        cache_read_usd_per_mtok: input * read_share,
        cache_write_usd_per_mtok: input * 1.25,
    }
}

/// A row whose cache writes cost plain input (OpenAI before GPT-5.6, Gemini's implicit
/// caching), with its published cached-input rate. A model with no cached rate (`*-pro`) reads
/// at full input, `cached = input`.
const fn free_writes(prefix: &'static str, input: f64, output: f64, cached: f64) -> ModelPrice {
    ModelPrice {
        prefix,
        prompt_usd_per_mtok: input,
        completion_usd_per_mtok: output,
        cache_read_usd_per_mtok: cached,
        cache_write_usd_per_mtok: input,
    }
}

/// A GPT-5.6-or-later row: OpenAI publishes its cached-input and its 30-minute cache-write rate.
const fn openai_explicit(prefix: &'static str, input: f64, output: f64, cached: f64, write: f64) -> ModelPrice {
    ModelPrice {
        prefix,
        prompt_usd_per_mtok: input,
        completion_usd_per_mtok: output,
        cache_read_usd_per_mtok: cached,
        cache_write_usd_per_mtok: write,
    }
}

/// The Claude price table (USD per million tokens), Anthropic's first-party rates (verified
/// 2026-10-29 against Anthropic's pricing page, platform.claude.com/docs/en/about-claude/pricing,
/// base prices and the cache multipliers). Generations within a family are priced apart (Opus
/// 4.5+ costs a third of Opus 4.1), so each generation gets its own row ahead of the family's
/// catch-all. Haiku 5.5's row is its rate for prompts up to 100K tokens; past that it costs five
/// times as much, which this flat table doesn't model.
const ANTHROPIC_PRICES: &[ModelPrice] = &[
    claude("claude-fable-5-1", 10.00, 50.00, 0.025),
    claude("claude-fable-", 10.00, 50.00, 0.1),
    claude("claude-opus-5-5", 4.00, 20.00, 0.05),
    claude("claude-opus-5", 5.00, 25.00, 0.1),
    claude("claude-opus-4-8", 5.00, 25.00, 0.1),
    claude("claude-opus-4-7", 5.00, 25.00, 0.1),
    claude("claude-opus-4-6", 5.00, 25.00, 0.1),
    claude("claude-opus-4-5", 5.00, 25.00, 0.1),
    // Opus 4 and 4.1.
    claude("claude-opus", 15.00, 75.00, 0.1),
    claude("claude-sonnet-5-5", 2.00, 10.00, 0.05),
    claude("claude-sonnet-5", 2.00, 10.00, 0.1),
    claude("claude-sonnet", 3.00, 15.00, 0.1),
    claude("claude-haiku-5-5", 0.10, 0.50, 0.1),
    claude("claude-haiku-4-5", 1.00, 5.00, 0.1),
    claude("claude-3-5-haiku", 0.80, 4.00, 0.1),
    claude("claude-3-7-sonnet", 3.00, 15.00, 0.1),
    claude("claude-3-5-sonnet", 3.00, 15.00, 0.1),
];

/// OpenAI's standard-tier rates (verified 2026-10-29 against developers.openai.com/api/docs/pricing.md).
/// GPT-5.6 and later bill a 30-minute cache write at 1.25x input; earlier models write for free.
/// The GPT-5.4 and 5.5 rows are their rates under 272K tokens of context.
const OPENAI_PRICES: &[ModelPrice] = &[
    openai_explicit("gpt-6.1-sol", 2.00, 10.00, 0.10, 2.50),
    openai_explicit("gpt-6.1-terra", 1.00, 5.00, 0.05, 1.25),
    openai_explicit("gpt-6-astra", 10.00, 50.00, 1.00, 12.50),
    openai_explicit("gpt-6-sol", 2.00, 10.00, 0.20, 2.50),
    openai_explicit("gpt-6-luna", 0.10, 0.50, 0.01, 0.125),
    openai_explicit("gpt-5.6-sol", 4.00, 20.00, 0.40, 5.00),
    openai_explicit("gpt-5.6-terra", 2.00, 12.00, 0.20, 2.50),
    openai_explicit("gpt-5.6-luna", 0.20, 1.20, 0.02, 0.25),
    free_writes("gpt-5.5-pro", 30.00, 180.00, 30.00),
    free_writes("gpt-5.5", 5.00, 30.00, 0.50),
    free_writes("gpt-5.4-pro", 30.00, 180.00, 30.00),
    free_writes("gpt-5.4-mini", 0.75, 4.50, 0.075),
    free_writes("gpt-5.4-nano", 0.20, 1.25, 0.02),
    free_writes("gpt-5.4", 2.50, 15.00, 0.25),
    free_writes("gpt-5.3-codex", 1.75, 14.00, 0.175),
    free_writes("gpt-5.2-pro", 21.00, 168.00, 21.00),
    free_writes("gpt-5.2", 1.75, 14.00, 0.175),
    free_writes("gpt-5.1", 1.25, 10.00, 0.125),
    free_writes("gpt-5-mini", 0.25, 2.00, 0.025),
    free_writes("gpt-5-nano", 0.05, 0.40, 0.005),
    free_writes("gpt-5-pro", 15.00, 120.00, 15.00),
    free_writes("gpt-5", 1.25, 10.00, 0.125),
    free_writes("gpt-4o-mini", 0.15, 0.60, 0.075),
    free_writes("gpt-4o", 2.50, 10.00, 1.25),
    free_writes("gpt-4.1-mini", 0.40, 1.60, 0.10),
    free_writes("gpt-4.1-nano", 0.10, 0.40, 0.025),
    free_writes("gpt-4.1", 2.00, 8.00, 0.50),
    free_writes("o4-mini", 1.10, 4.40, 0.275),
];

/// Gemini's paid-tier rates for prompts up to 200K tokens (verified 2026-10-29 against
/// ai.google.dev/gemini-api/docs/pricing). Implicit caching writes nothing extra. 2.0 Flash's
/// cached rate wasn't on the page, so its reads price at full input: a high estimate rather
/// than a guessed discount.
const GEMINI_PRICES: &[ModelPrice] = &[
    free_writes("gemini-2.5-flash-lite", 0.10, 0.40, 0.01),
    free_writes("gemini-2.5-flash", 0.30, 2.50, 0.03),
    free_writes("gemini-2.5-pro", 1.25, 10.00, 0.125),
    free_writes("gemini-2.0-flash", 0.10, 0.40, 0.10),
];

/// Price one completed call's usage. Local ⇒ free + priced; a known cloud model ⇒
/// estimated + priced; an unknown cloud model ⇒ 0 + unpriced (shown "unknown", never $0).
///
/// `usage.prompt_tokens` is the whole input, the cached share included (genai normalizes every
/// adapter that way), so the plain-input part is what's left after the cache reads and writes.
pub fn price_call(provider: ProviderTag, model: &str, usage: AgentUsage) -> PricedCost {
    if provider == ProviderTag::Local {
        return PricedCost {
            cost_micros: 0,
            priced: true,
        };
    }
    let Some(price) = lookup_price(provider, model) else {
        return PricedCost {
            cost_micros: 0,
            priced: false,
        };
    };
    let read = f64::from(usage.cache_read_tokens);
    let write = f64::from(usage.cache_write_tokens);
    let plain = f64::from(
        usage
            .prompt_tokens
            .saturating_sub(usage.cache_read_tokens)
            .saturating_sub(usage.cache_write_tokens),
    );
    // cost_micros = tokens * usd_per_mtok (derivation: micro-USD = USD * 1e6, and
    // USD = tokens/1e6 * usd_per_mtok, so the 1e6 factors cancel). Round to the nearest
    // micro-USD.
    let micros = plain * price.prompt_usd_per_mtok
        + read * price.cache_read_usd_per_mtok
        + write * price.cache_write_usd_per_mtok
        + f64::from(usage.completion_tokens) * price.completion_usd_per_mtok;
    PricedCost {
        cost_micros: micros.round() as i64,
        priced: true,
    }
}

/// Find the price for a provider+model by longest-matching id prefix, or `None` for an
/// unknown model. The table's ordering guarantees a more-specific prefix is tried first.
fn lookup_price(provider: ProviderTag, model: &str) -> Option<&'static ModelPrice> {
    let (table, model) = match provider {
        ProviderTag::Anthropic => (ANTHROPIC_PRICES, model.to_string()),
        // The Responses API is the same OpenAI catalog, priced identically.
        ProviderTag::OpenAiResponses => (OPENAI_PRICES, model.to_string()),
        ProviderTag::OpenAi => vendor_routed(model).unwrap_or((OPENAI_PRICES, model.to_string())),
        ProviderTag::Gemini => (GEMINI_PRICES, model.to_string()),
        ProviderTag::Local => return None,
    };
    table.iter().find(|p| model.starts_with(p.prefix))
}

/// A chat-completions id that names its maker the way OpenRouter does (`anthropic/claude-sonnet-5.5`,
/// `openai/gpt-6.1-sol`, `google/gemini-2.5-flash`), as that maker's table and id. OpenRouter
/// passes the maker's own price through. Its Claude ids write versions with dots where
/// Anthropic's use dashes. A `~anthropic/…-latest` alias names no version, so it stays unpriced.
fn vendor_routed(model: &str) -> Option<(&'static [ModelPrice], String)> {
    let (vendor, id) = model.split_once('/')?;
    match vendor {
        "anthropic" => Some((ANTHROPIC_PRICES, id.replace('.', "-"))),
        "openai" => Some((OPENAI_PRICES, id.to_string())),
        "google" => Some((GEMINI_PRICES, id.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
