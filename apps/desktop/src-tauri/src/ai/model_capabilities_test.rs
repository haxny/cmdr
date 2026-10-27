//! Each provider's model list, parsed from a recorded answer (`model_capabilities_fixtures/`,
//! trimmed from live `GET …/models` responses on 2026-10-27 to a few representative models).

use super::*;

fn parsed(protocol: ModelListProtocol, fixture: &str) -> ModelCatalog {
    parse_catalog(protocol, fixture)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn anthropic_reports_window_output_effort_and_thinking() {
    let catalog = parsed(
        ModelListProtocol::Anthropic,
        include_str!("model_capabilities_fixtures/anthropic.json"),
    );
    let opus = &catalog["claude-opus-5-5"];
    assert_eq!(opus.context_window_tokens, Some(1_000_000));
    assert_eq!(opus.max_output_tokens, Some(128_000));
    assert_eq!(opus.reasoning.supported, Some(true));
    assert_eq!(
        opus.reasoning.effort_levels,
        strings(&["low", "medium", "high", "xhigh", "max"]),
        "levels low to high, without the section's own `supported` flag"
    );
    assert_eq!(opus.reasoning.thinking_modes, strings(&["adaptive"]));

    let sonnet = &catalog["claude-sonnet-4-6"];
    assert_eq!(
        sonnet.reasoning.thinking_modes,
        strings(&["adaptive", "disabled", "enabled"])
    );

    let old = &catalog["claude-opus-4-5-20251101"];
    assert_eq!(
        (old.context_window_tokens, old.max_output_tokens),
        (Some(200_000), Some(64_000))
    );
}

#[test]
fn gemini_reports_limits_and_thinking_under_bare_ids() {
    let catalog = parsed(
        ModelListProtocol::Gemini,
        include_str!("model_capabilities_fixtures/gemini.json"),
    );
    let flash = &catalog["gemini-3.8-flash"];
    assert_eq!(flash.context_window_tokens, Some(1_048_576));
    assert_eq!(flash.max_output_tokens, Some(65_536));
    assert_eq!(flash.reasoning.supported, Some(true));
    assert_eq!(catalog["gemini-2.5-flash-image"].context_window_tokens, Some(32_768));
    assert_eq!(
        catalog["gemini-2.5-flash-image"].reasoning.supported, None,
        "a model without the field says nothing about thinking"
    );
}

#[test]
fn groq_names_it_context_window_and_lists_reasoning_as_a_feature() {
    let catalog = parsed(
        ModelListProtocol::OpenAiCompatible,
        include_str!("model_capabilities_fixtures/groq.json"),
    );
    let oss = &catalog["openai/gpt-oss-120b"];
    assert_eq!(oss.context_window_tokens, Some(131_072));
    assert_eq!(oss.max_output_tokens, Some(65_536));
    assert_eq!(oss.reasoning.supported, Some(true));
}

#[test]
fn fireworks_names_it_context_length_and_may_leave_it_out() {
    let catalog = parsed(
        ModelListProtocol::OpenAiCompatible,
        include_str!("model_capabilities_fixtures/fireworks.json"),
    );
    assert_eq!(
        catalog["accounts/fireworks/models/glm-5p3"].context_window_tokens,
        Some(1_048_576)
    );
    assert_eq!(
        catalog["accounts/fireworks/models/qwen3p8-max"].context_window_tokens, None,
        "a null window is no window, never zero"
    );
}

#[test]
fn mistral_names_it_max_context_length_with_a_reasoning_flag() {
    let catalog = parsed(
        ModelListProtocol::OpenAiCompatible,
        include_str!("model_capabilities_fixtures/mistral.json"),
    );
    let medium = &catalog["mistral-medium-latest"];
    assert_eq!(medium.context_window_tokens, Some(262_144));
    assert_eq!(medium.reasoning.supported, Some(true));
    assert_eq!(catalog["codestral-2508"].reasoning.supported, Some(false));
}

#[test]
fn openrouter_reports_its_parameters_and_reasoning_block() {
    let catalog = parsed(
        ModelListProtocol::OpenRouter,
        include_str!("model_capabilities_fixtures/openrouter.json"),
    );
    let pro = &catalog["openai/gpt-6.1-terra-pro"];
    assert_eq!(pro.context_window_tokens, Some(1_050_000));
    assert_eq!(pro.max_output_tokens, Some(128_000));
    assert_eq!(pro.reasoning.supported, Some(true));
    assert_eq!(pro.reasoning.mandatory, Some(true));
    assert_eq!(
        pro.reasoning.effort_levels,
        strings(&["low", "medium", "high", "xhigh", "max"])
    );
    assert_eq!(pro.reasoning.default_effort.as_deref(), Some("medium"));
    assert!(pro.supported_parameters.iter().any(|p| p == "reasoning_effort"));

    let llama = &catalog["meta-llama/llama-3.3-70b-instruct"];
    assert_eq!(llama.context_window_tokens, Some(131_072));
    assert_eq!(
        llama.reasoning.supported,
        Some(false),
        "OpenRouter lists its parameters, and reasoning isn't one"
    );
}

#[test]
fn openai_reports_no_window_so_nothing_is_claimed() {
    let catalog = parsed(
        ModelListProtocol::OpenAiCompatible,
        include_str!("model_capabilities_fixtures/openai.json"),
    );
    assert!(!catalog.is_empty(), "the models are listed");
    assert!(
        catalog.values().all(|c| *c == ModelCapabilities::default()),
        "but with nothing about them, so the budget falls back to its table"
    );
}

/// Together answers a bare array (per its API reference; not verified live, no key).
#[test]
fn a_bare_array_list_parses_too() {
    let catalog = parsed(
        ModelListProtocol::OpenAiCompatible,
        r#"[{"id":"meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8","context_length":1048576}]"#,
    );
    assert_eq!(
        catalog["meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"].context_window_tokens,
        Some(1_048_576)
    );
}

#[test]
fn a_body_it_cannot_read_is_an_empty_catalog() {
    assert!(parsed(ModelListProtocol::Anthropic, "not json").is_empty());
    assert!(parsed(ModelListProtocol::Perplexity, r#"{"data":[{"id":"sonar"}]}"#).is_empty());
}

#[test]
fn an_undated_alias_finds_its_one_dated_snapshot() {
    let mut catalog = ModelCatalog::new();
    let window = |tokens| ModelCapabilities {
        context_window_tokens: Some(tokens),
        ..ModelCapabilities::default()
    };
    catalog.insert("claude-opus-4-5-20251101".into(), window(200_000));
    catalog.insert("claude-opus-4-5-extra".into(), window(1));
    assert_eq!(
        find_model(&catalog, "claude-opus-4-5").and_then(|c| c.context_window_tokens),
        Some(200_000)
    );
    assert!(
        find_model(&catalog, "claude-opus-4").is_none(),
        "a shorter stem isn't an alias"
    );

    catalog.insert("claude-opus-4-5-20260101".into(), window(1_000_000));
    assert!(
        find_model(&catalog, "claude-opus-4-5").is_none(),
        "two snapshots under one alias: which is the provider's call, so no guess"
    );
}

#[test]
fn a_refresh_waits_for_staleness_and_after_a_failure() {
    let now = Instant::now();
    let later = |by: Duration| now + by;

    assert!(CatalogSlot::default().wants_refresh(now), "nothing cached yet");

    let fetched = CatalogSlot {
        catalog: Some((ModelCatalog::new(), now)),
        last_attempt: Some(now),
        in_flight: false,
    };
    assert!(!fetched.wants_refresh(later(Duration::from_secs(60))));
    assert!(fetched.wants_refresh(later(REFRESH_AFTER + Duration::from_secs(1))));

    let failed = CatalogSlot {
        catalog: None,
        last_attempt: Some(now),
        in_flight: false,
    };
    assert!(!failed.wants_refresh(later(Duration::from_secs(60))), "a failure waits");
    assert!(failed.wants_refresh(later(RETRY_AFTER + Duration::from_secs(1))));

    let running = CatalogSlot {
        in_flight: true,
        ..CatalogSlot::default()
    };
    assert!(!running.wants_refresh(now), "one refresh at a time");
}

/// The key rides every background fetch, so an endpoint seen only through a lookup (a custom base
/// URL arrives once per keystroke, and each prefix is a real host someone could own) is never
/// fetched from. Only a connection check's success, or the endpoint Cmdr started with, trusts one.
#[test]
fn a_background_refresh_never_goes_to_an_untrusted_endpoint() {
    let now = Instant::now();
    let mut catalogs = Catalogs::default();
    let typed = CatalogKey::new("custom", "https://api.mycompany.co");
    assert!(
        !catalogs.begin_refresh(&typed, now),
        "a prefix seen mid-typing is never fetched from"
    );
    assert!(catalogs.slots.is_empty(), "and leaves nothing behind");

    let checked = CatalogKey::new("custom", "https://api.mycompany.com/v1");
    catalogs.store(checked.clone(), ModelCatalog::new(), now, None);
    assert!(
        catalogs.begin_refresh(&checked, now + REFRESH_AFTER + Duration::from_secs(1)),
        "a checked endpoint refreshes once stale"
    );
}

#[test]
fn storing_a_catalog_evicts_every_endpoint_but_it_and_the_configured_one() {
    let now = Instant::now();
    let mut catalogs = Catalogs::default();
    let configured = CatalogKey::new("anthropic", "https://api.anthropic.com/v1");
    let tried = CatalogKey::new("openai", "https://api.openai.com/v1");
    let latest = CatalogKey::new("groq", "https://api.groq.com/openai/v1");
    catalogs.store(configured.clone(), ModelCatalog::new(), now, Some(&configured));
    catalogs.store(tried.clone(), ModelCatalog::new(), now, Some(&configured));
    catalogs.store(latest.clone(), ModelCatalog::new(), now, Some(&configured));

    let mut kept: Vec<_> = catalogs.slots.keys().map(|k| k.provider_id.clone()).collect();
    kept.sort();
    assert_eq!(kept, vec!["anthropic".to_string(), "groq".to_string()]);
    assert!(
        !catalogs.trusted.contains(&tried),
        "an evicted endpoint is no longer trusted either"
    );
}

#[test]
fn settings_shows_the_providers_window_or_the_tables_guess() {
    let reported = ModelCapabilities {
        context_window_tokens: Some(1_000_000),
        max_output_tokens: Some(128_000),
        ..ModelCapabilities::default()
    };
    let from_provider = limits_for("claude-sonnet-4-5", Some(&reported));
    assert_eq!(from_provider.context_window_tokens, Some(1_000_000));
    assert_eq!(from_provider.max_output_tokens, Some(128_000));
    assert_eq!(from_provider.source, Some(AiModelLimitsSource::Provider));

    let from_table = limits_for("gpt-4o-mini", None);
    assert_eq!(from_table.context_window_tokens, Some(128_000));
    assert_eq!(from_table.max_output_tokens, None, "the table never guesses the reply");
    assert_eq!(from_table.source, Some(AiModelLimitsSource::BuiltInTable));

    let unknown = limits_for("llama3.2", None);
    assert_eq!((unknown.context_window_tokens, unknown.source), (None, None));
}
