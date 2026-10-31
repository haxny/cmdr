use super::*;

fn plain(prompt_tokens: u32, completion_tokens: u32) -> AgentUsage {
    AgentUsage {
        prompt_tokens,
        completion_tokens,
        ..AgentUsage::default()
    }
}

#[test]
fn local_is_free_and_priced() {
    let cost = price_call(ProviderTag::Local, "any-local-model", plain(1000, 500));
    assert_eq!(
        cost,
        PricedCost {
            cost_micros: 0,
            priced: true
        }
    );
}

#[test]
fn unknown_cloud_model_is_unpriced_never_zero_cost_silently() {
    // The honest miss-path: an unknown cloud model records tokens (elsewhere) but its
    // cost is UNKNOWN, flagged `priced = false`, never a silent $0.
    let cost = price_call(ProviderTag::OpenAi, "some-future-model-9000", plain(1000, 500));
    assert!(!cost.priced, "an unknown model must be flagged unpriced");
    assert_eq!(cost.cost_micros, 0);
}

#[test]
fn known_openai_model_estimates_cost() {
    // gpt-4o-mini: $0.15/Mtok prompt, $0.60/Mtok completion.
    // 1000 * 0.15 + 500 * 0.60 = 150 + 300 = 450 micro-USD ($0.00045).
    let cost = price_call(ProviderTag::OpenAi, "gpt-4o-mini", plain(1000, 500));
    assert!(cost.priced);
    assert_eq!(cost.cost_micros, 450);
}

#[test]
fn longer_prefix_wins_over_family_stem() {
    // `gpt-4o-mini` must not match the pricier `gpt-4o` entry.
    let mini = price_call(ProviderTag::OpenAi, "gpt-4o-mini-2024-07-18", plain(1_000_000, 0));
    assert_eq!(mini.cost_micros, 150_000, "gpt-4o-mini prompt is $0.15/Mtok");
    let base = price_call(ProviderTag::OpenAi, "gpt-4o-2024-11-20", plain(1_000_000, 0));
    assert_eq!(base.cost_micros, 2_500_000, "gpt-4o prompt is $2.50/Mtok");
}

/// Each Claude generation hits its own row, not its family's catch-all: Opus 5.5 once
/// priced at the $15 Opus 4.1 rate, almost four times its real cost.
#[test]
fn claude_generations_price_apart_within_a_family() {
    let prompt_cost = |model| price_call(ProviderTag::Anthropic, model, plain(1_000_000, 0)).cost_micros;
    assert_eq!(prompt_cost("claude-opus-5-5"), 4_000_000);
    assert_eq!(prompt_cost("claude-opus-5"), 5_000_000);
    assert_eq!(prompt_cost("claude-opus-4-5-20251101"), 5_000_000);
    assert_eq!(prompt_cost("claude-opus-4-1"), 15_000_000);
    assert_eq!(prompt_cost("claude-sonnet-5-5"), 2_000_000);
    assert_eq!(prompt_cost("claude-sonnet-4-6"), 3_000_000);
    assert_eq!(prompt_cost("claude-haiku-5-5"), 100_000);
    assert_eq!(prompt_cost("claude-haiku-4-5-20251001"), 1_000_000);
    assert_eq!(prompt_cost("claude-fable-5-1"), 10_000_000);
}

#[test]
fn known_anthropic_and_gemini_models_are_priced() {
    assert!(price_call(ProviderTag::Anthropic, "claude-sonnet-4-5", plain(100, 100)).priced);
    assert!(price_call(ProviderTag::Gemini, "gemini-2.5-flash", plain(100, 100)).priced);
}

/// A second call in a thread reads the first one's prompt back from the cache, and the meter
/// prices that share at the cache rate (#770). Without it a cached chat priced as if every
/// round re-billed the whole thread.
#[test]
fn cache_reads_and_writes_price_at_their_own_rates() {
    // Sonnet 4.6: $3 input, reads 0.1x ($0.30), 5-minute writes 1.25x ($3.75).
    let sonnet = |read, write| {
        let usage = AgentUsage {
            prompt_tokens: 1_000_000,
            completion_tokens: 0,
            cache_read_tokens: read,
            cache_write_tokens: write,
        };
        price_call(ProviderTag::Anthropic, "claude-sonnet-4-6", usage).cost_micros
    };
    assert_eq!(sonnet(1_000_000, 0), 300_000);
    assert_eq!(sonnet(0, 1_000_000), 3_750_000);
    // The uncached remainder still pays plain input: 600K read, 100K written, 300K plain.
    assert_eq!(sonnet(600_000, 100_000), 180_000 + 375_000 + 900_000);
}

#[test]
fn claude_cache_reads_follow_each_models_own_share() {
    let read_cost = |model| {
        let usage = AgentUsage {
            prompt_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            ..AgentUsage::default()
        };
        price_call(ProviderTag::Anthropic, model, usage).cost_micros
    };
    assert_eq!(read_cost("claude-opus-5-5"), 200_000, "0.05x of $4");
    assert_eq!(read_cost("claude-sonnet-5-5"), 100_000, "0.05x of $2");
    assert_eq!(read_cost("claude-sonnet-5"), 200_000, "0.1x of $2");
    assert_eq!(read_cost("claude-fable-5-1"), 250_000, "0.025x of $10");
    assert_eq!(read_cost("claude-fable-5"), 1_000_000, "0.1x of $10");
}

#[test]
fn gpt_5_6_and_later_pay_for_cache_writes_and_earlier_ones_do_not() {
    let usage = |read, write| AgentUsage {
        prompt_tokens: 1_000_000,
        cache_read_tokens: read,
        cache_write_tokens: write,
        ..AgentUsage::default()
    };
    let sol = |usage| price_call(ProviderTag::OpenAiResponses, "gpt-6.1-sol", usage).cost_micros;
    assert_eq!(sol(usage(1_000_000, 0)), 100_000, "cached input $0.10");
    assert_eq!(sol(usage(0, 1_000_000)), 2_500_000, "30-minute write $2.50");
    let older = |usage| price_call(ProviderTag::OpenAiResponses, "gpt-5.5", usage).cost_micros;
    assert_eq!(older(usage(1_000_000, 0)), 500_000, "cached input $0.50");
    assert_eq!(older(usage(0, 1_000_000)), 5_000_000, "a write is plain input");
}

#[test]
fn new_openai_families_are_priced() {
    for model in [
        "gpt-6.1-sol",
        "gpt-6-luna",
        "gpt-5.6-terra",
        "gpt-5.5",
        "gpt-5.4-mini",
        "gpt-5",
    ] {
        assert!(
            price_call(ProviderTag::OpenAiResponses, model, plain(100, 100)).priced,
            "{model}"
        );
    }
    let prompt_cost = |model| price_call(ProviderTag::OpenAiResponses, model, plain(1_000_000, 0)).cost_micros;
    assert_eq!(
        prompt_cost("gpt-6.1-terra"),
        1_000_000,
        "not the gpt-6 rows ahead of it"
    );
    assert_eq!(prompt_cost("gpt-5.4-mini-2026-03-17"), 750_000, "not gpt-5.4");
    assert_eq!(prompt_cost("gpt-5-mini"), 250_000, "not gpt-5");
}

/// #801: an OpenRouter call to Claude names the maker in its id and writes the version with
/// dots, and OpenRouter passes the maker's price through, so it prices off Anthropic's row.
#[test]
fn openrouter_ids_price_off_their_makers_table() {
    let usage = AgentUsage {
        prompt_tokens: 1_000_000,
        cache_read_tokens: 1_000_000,
        ..AgentUsage::default()
    };
    let claude = price_call(ProviderTag::OpenAi, "anthropic/claude-sonnet-5.5", usage);
    assert_eq!(claude.cost_micros, 100_000, "Sonnet 5.5's 0.05x cache read");
    assert!(price_call(ProviderTag::OpenAi, "openai/gpt-6.1-sol", plain(100, 100)).priced);
    assert!(price_call(ProviderTag::OpenAi, "google/gemini-2.5-flash", plain(100, 100)).priced);
    // An alias names no version, and an open-weight id is no maker's table.
    assert!(!price_call(ProviderTag::OpenAi, "~anthropic/claude-sonnet-latest", plain(100, 100)).priced);
    assert!(!price_call(ProviderTag::OpenAi, "openai/gpt-oss-120b", plain(100, 100)).priced);
}
