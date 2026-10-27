//! What each cloud model can take, as its provider reports it: the context window, the longest
//! reply, and the reasoning knobs. Ask Cmdr sizes its prompt from the window
//! (`agent::chat::budget`), and Settings › AI shows it.
//!
//! **Fetched from the same model list the connection check reads** (`connection_check::
//! list_models`): Anthropic `max_input_tokens` / `max_tokens` / `capabilities`, Gemini
//! `inputTokenLimit` / `outputTokenLimit` / `thinking`, and on the OpenAI-shaped lists
//! whichever of `context_length` (Fireworks, OpenRouter, Together), `context_window` (Groq),
//! or `max_context_length` (Mistral) the host fills. OpenAI, DeepSeek, xAI, and Qwen report no
//! window, so a model of theirs has no entry here and the budget falls back to its table.
//!
//! **Never blocks a send.** A lookup reads the in-memory cache and, when the entry is missing
//! or older than [`REFRESH_AFTER`], starts ONE background refresh; the send goes on with what
//! is there (the table, on the first send after launch). `configure_ai` refreshes on every
//! provider, key, model, or base-URL change, and a connection check stores what it already
//! fetched. The cache is keyed by (provider id, base URL), the two things that decide which
//! catalog answers.
//!
//! **Egress**: a model-list call carries no user data, but it reaches the provider, so a
//! refresh asks the managed policy and cloud consent first, exactly like the connection check.
//! Two callers start one: Ask Cmdr's lookup only while `ai.provider` is `cloud`, and
//! Settings › AI for the provider being set up (like the connection check, whatever
//! `ai.provider` says).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tauri_specta::Event;

use super::connection_check::ModelListProtocol;
use crate::ignore_poison::IgnorePoison;

/// How long a fetched catalog stays fresh. Windows change with a model release, not hourly.
const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// How long to wait after a refresh that failed (offline, key rejected) before trying again,
/// so a lookup per send doesn't turn into a request per send.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// What one model can take, typed from the provider's own fields. Every field is optional or
/// empty because no provider fills them all; `None` means "not reported", never "zero".
///
/// Effort and thinking are carried for the effort picker that builds on this record; the
/// budget reads only the window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// The most input tokens one request may carry.
    pub context_window_tokens: Option<u32>,
    /// The longest reply the model can produce.
    pub max_output_tokens: Option<u32>,
    pub reasoning: ReasoningCapabilities,
    /// OpenRouter's `supported_parameters`, verbatim (`reasoning`, `reasoning_effort`,
    /// `tools`, …): what a request to this model may set.
    pub supported_parameters: Vec<String>,
}

/// How a model reasons, in the provider's own words. Values stay strings rather than an enum
/// because each provider names its levels differently and adds new ones without notice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReasoningCapabilities {
    /// Whether the model reasons at all: Anthropic `capabilities.thinking.supported`, Gemini
    /// `thinking`, Mistral `capabilities.reasoning`, Groq `supported_features` naming
    /// `reasoning`, OpenRouter `supported_parameters` naming it.
    pub supported: Option<bool>,
    /// The model always reasons and can't be told not to (OpenRouter `reasoning.mandatory`).
    pub mandatory: Option<bool>,
    /// The effort levels it accepts, low to high where the provider orders them: Anthropic's
    /// supported `capabilities.effort.*`, OpenRouter's `reasoning.supported_efforts`.
    pub effort_levels: Vec<String>,
    /// The effort it uses when none is asked for (OpenRouter `reasoning.default_effort`).
    pub default_effort: Option<String>,
    /// Anthropic's supported `capabilities.thinking.types` (`enabled`, `adaptive`,
    /// `disabled`, …): which `thinking` request shapes the model accepts.
    pub thinking_modes: Vec<String>,
}

/// Every model one provider list described, keyed by the id a request names it by.
pub type ModelCatalog = HashMap<String, ModelCapabilities>;

/// Which catalog answers: the same model id can mean different limits on different hosts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CatalogKey {
    provider_id: String,
    base_url: String,
}

impl CatalogKey {
    fn new(provider_id: &str, base_url: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[derive(Debug, Default)]
struct CatalogSlot {
    catalog: Option<(ModelCatalog, Instant)>,
    last_attempt: Option<Instant>,
    in_flight: bool,
}

impl CatalogSlot {
    /// Whether a lookup at `now` should start a refresh: nothing running, the catalog missing
    /// or stale, and no failed attempt in the last [`RETRY_AFTER`].
    fn wants_refresh(&self, now: Instant) -> bool {
        if self.in_flight {
            return false;
        }
        let fresh = self
            .catalog
            .as_ref()
            .is_some_and(|(_, fetched)| now.duration_since(*fetched) < REFRESH_AFTER);
        let retried_recently = self
            .last_attempt
            .is_some_and(|attempt| now.duration_since(attempt) < RETRY_AFTER);
        !fresh && !retried_recently
    }
}

static CATALOGS: Mutex<Option<HashMap<CatalogKey, CatalogSlot>>> = Mutex::new(None);

/// Tells the windows a catalog arrived, so Settings › AI re-reads the window it shows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiModelCapabilitiesChanged;

/// Keep what a model list answered, for (`provider_id`, `base_url`). Called by the connection
/// check and by a refresh.
pub(super) fn store<R: Runtime>(app: &AppHandle<R>, provider_id: &str, base_url: &str, catalog: ModelCatalog) {
    let now = Instant::now();
    {
        let mut catalogs = CATALOGS.lock_ignore_poison();
        let slot = catalogs
            .get_or_insert_with(HashMap::new)
            .entry(CatalogKey::new(provider_id, base_url))
            .or_default();
        slot.catalog = Some((catalog, now));
        slot.last_attempt = Some(now);
        slot.in_flight = false;
    }
    if let Err(e) = AiModelCapabilitiesChanged.emit(app) {
        log::debug!("couldn't announce new model capabilities: {e}");
    }
}

/// What the cache knows about `model` on (`provider_id`, `base_url`), with no I/O. Pair it with
/// [`refresh`] where staying current matters.
pub fn lookup(provider_id: &str, base_url: &str, model: &str) -> Option<ModelCapabilities> {
    let catalogs = CATALOGS.lock_ignore_poison();
    let slot = catalogs.as_ref()?.get(&CatalogKey::new(provider_id, base_url))?;
    let (catalog, _) = slot.catalog.as_ref()?;
    find_model(catalog, model).cloned()
}

/// What the provider reports for `model` on the configured cloud provider, refreshing in the
/// background when the cache is stale. `None` when AI isn't on Cloud or nothing was reported.
pub fn for_configured_cloud_model<R: Runtime>(app: &AppHandle<R>, model: &str) -> Option<ModelCapabilities> {
    if super::state::get_provider() != "cloud" {
        return None;
    }
    let provider_id = super::state::get_cloud_provider_id();
    let (_, base_url, _) = super::state::get_cloud_config();
    refresh(app, &provider_id, &base_url, Refresh::IfStale);
    lookup(&provider_id, &base_url, model)
}

/// When [`refresh`] should fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Only when the catalog is missing or stale, and no failed attempt is recent.
    IfStale,
    /// Now, unless a fetch is already running: the key or endpoint just changed, so neither
    /// the cached catalog nor the last failure says anything about the new one.
    Now,
}

/// Start one background refresh of (`provider_id`, `base_url`)'s catalog, as `when` says.
/// Returns at once; the result lands in the cache and announces itself.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, provider_id: &str, base_url: &str, when: Refresh) {
    if provider_id.is_empty() || base_url.is_empty() {
        return;
    }
    let key = CatalogKey::new(provider_id, base_url);
    {
        let mut catalogs = CATALOGS.lock_ignore_poison();
        let slot = catalogs
            .get_or_insert_with(HashMap::new)
            .entry(key.clone())
            .or_default();
        let go = match when {
            Refresh::IfStale => slot.wants_refresh(Instant::now()),
            Refresh::Now => !slot.in_flight,
        };
        if !go {
            return;
        }
        slot.in_flight = true;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let fetched = fetch_catalog(&app, &key).await;
        match fetched {
            Some(catalog) => {
                log::debug!(
                    "model capabilities for {}: {} models reported",
                    key.provider_id,
                    catalog.len()
                );
                store(&app, &key.provider_id, &key.base_url, catalog);
            }
            None => {
                let mut catalogs = CATALOGS.lock_ignore_poison();
                if let Some(slot) = catalogs.as_mut().and_then(|c| c.get_mut(&key)) {
                    slot.in_flight = false;
                    slot.last_attempt = Some(Instant::now());
                }
            }
        }
    });
}

/// One gated fetch: the policy and consent first (the same two gates the connection check
/// asks), then the key from the secret store, then the list. `None` on any refusal or failure,
/// which the caller records so the retry waits.
async fn fetch_catalog<R: Runtime>(app: &AppHandle<R>, key: &CatalogKey) -> Option<ModelCatalog> {
    if super::connection_check::refuse_by_policy(&*crate::managed_policy::for_egress().await, &key.base_url).is_some() {
        return None;
    }
    if !super::cloud_consent::cloud_consent_from_app(app) {
        return None;
    }
    let (api_key, _) = super::api_keys::read_for_backend(&key.provider_id);
    let protocol = ModelListProtocol::for_provider(&key.provider_id);
    match super::connection_check::list_models(&key.base_url, &api_key, protocol).await {
        Ok(listed) => Some(listed.capabilities),
        Err(problem) => {
            log::debug!(
                "model capabilities for {}: not refreshed ({problem:?})",
                key.provider_id
            );
            None
        }
    }
}

/// `model` in `catalog`: the exact id, else the one dated snapshot it is an alias of
/// (`claude-opus-4-5` names `claude-opus-4-5-20251101`, which is what Anthropic lists).
fn find_model<'a>(catalog: &'a ModelCatalog, model: &str) -> Option<&'a ModelCapabilities> {
    if let Some(found) = catalog.get(model) {
        return Some(found);
    }
    let mut snapshots = catalog.iter().filter(|(id, _)| {
        id.strip_prefix(model)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|date| !date.is_empty() && date.chars().all(|c| c.is_ascii_digit()))
    });
    let (_, only) = snapshots.next()?;
    // Two snapshots under one alias: which one the alias points at is the provider's call, and
    // guessing could pick the smaller window. Say nothing instead.
    snapshots.next().is_none().then_some(only)
}

/// Every model a list body describes, parsed the way `protocol` shapes it. A body this doesn't
/// understand yields an empty catalog, never an error: the list served its purpose for the key
/// check either way.
pub(super) fn parse_catalog(protocol: ModelListProtocol, body: &str) -> ModelCatalog {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return ModelCatalog::new();
    };
    match protocol {
        ModelListProtocol::Anthropic => entries(&value, "data")
            .filter_map(|entry| Some((str_at(entry, &["id"])?, anthropic_capabilities(entry))))
            .collect(),
        ModelListProtocol::Gemini => entries(&value, "models")
            .filter_map(|entry| {
                let name = str_at(entry, &["name"])?;
                let id = name.strip_prefix("models/").unwrap_or(&name).to_string();
                Some((id, gemini_capabilities(entry)))
            })
            .collect(),
        ModelListProtocol::OpenAiCompatible | ModelListProtocol::OpenRouter => entries(&value, "data")
            .filter_map(|entry| Some((str_at(entry, &["id"])?, openai_shaped_capabilities(entry))))
            .collect(),
        ModelListProtocol::Perplexity => ModelCatalog::new(),
    }
}

/// The entries of a list: `{ "<field>": [...] }`, or a bare array (Together).
fn entries<'a>(value: &'a Value, field: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(field)
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .into_iter()
        .flatten()
}

fn anthropic_capabilities(entry: &Value) -> ModelCapabilities {
    let capabilities = entry.get("capabilities");
    let supported_keys = |section: &str| -> Vec<String> {
        capabilities
            .and_then(|c| c.get(section))
            .and_then(Value::as_object)
            .map(|levels| {
                levels
                    .iter()
                    .filter(|(_, level)| bool_at(level, &["supported"]) == Some(true))
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    // `effort` lists its levels beside its own `supported` flag; the flag isn't a level.
    let effort_levels = if capabilities.and_then(|c| bool_at(c, &["effort", "supported"])) == Some(true) {
        order_efforts(
            supported_keys("effort")
                .into_iter()
                .filter(|k| k != "supported")
                .collect(),
        )
    } else {
        Vec::new()
    };
    let thinking_modes = capabilities
        .and_then(|c| c.get("thinking"))
        .map(|thinking| {
            let mut modes: Vec<String> = thinking
                .get("types")
                .and_then(Value::as_object)
                .map(|types| {
                    types
                        .iter()
                        .filter(|(_, t)| bool_at(t, &["supported"]) == Some(true))
                        .map(|(name, _)| name.clone())
                        .collect()
                })
                .unwrap_or_default();
            modes.sort();
            modes
        })
        .unwrap_or_default();
    ModelCapabilities {
        context_window_tokens: u32_at(entry, &["max_input_tokens"]),
        max_output_tokens: u32_at(entry, &["max_tokens"]),
        reasoning: ReasoningCapabilities {
            supported: capabilities.and_then(|c| bool_at(c, &["thinking", "supported"])),
            mandatory: None,
            effort_levels,
            default_effort: None,
            thinking_modes,
        },
        supported_parameters: Vec::new(),
    }
}

fn gemini_capabilities(entry: &Value) -> ModelCapabilities {
    ModelCapabilities {
        context_window_tokens: u32_at(entry, &["inputTokenLimit"]),
        max_output_tokens: u32_at(entry, &["outputTokenLimit"]),
        reasoning: ReasoningCapabilities {
            supported: bool_at(entry, &["thinking"]),
            ..ReasoningCapabilities::default()
        },
        supported_parameters: Vec::new(),
    }
}

/// The OpenAI-compatible hosts each name the window differently, and OpenRouter nests its
/// per-host figures under `top_provider`, so the first field present wins.
fn openai_shaped_capabilities(entry: &Value) -> ModelCapabilities {
    let first_u32 = |paths: &[&[&str]]| paths.iter().find_map(|path| u32_at(entry, path));
    let supported_parameters: Vec<String> = strings_at(entry, &["supported_parameters"]);
    let features: Vec<String> = strings_at(entry, &["supported_features"]);
    let reasoning_block = entry.get("reasoning").filter(|r| r.is_object());
    let supported = if let Some(flag) = bool_at(entry, &["capabilities", "reasoning"]) {
        Some(flag) // Mistral
    } else if reasoning_block.is_some() || supported_parameters.iter().any(|p| p == "reasoning") {
        Some(true) // OpenRouter
    } else if !supported_parameters.is_empty() {
        Some(false) // OpenRouter lists its parameters, and reasoning isn't one
    } else if features.iter().any(|f| f == "reasoning") {
        Some(true) // Groq
    } else if !features.is_empty() {
        Some(false)
    } else {
        None
    };
    ModelCapabilities {
        context_window_tokens: first_u32(&[
            &["context_length"],
            &["context_window"],
            &["max_context_length"],
            &["top_provider", "context_length"],
        ]),
        max_output_tokens: first_u32(&[
            &["top_provider", "max_completion_tokens"],
            &["max_completion_tokens"],
            &["max_output_length"],
        ]),
        reasoning: ReasoningCapabilities {
            supported,
            mandatory: reasoning_block.and_then(|r| bool_at(r, &["mandatory"])),
            effort_levels: reasoning_block
                .map(|r| order_efforts(strings_at(r, &["supported_efforts"])))
                .unwrap_or_default(),
            default_effort: reasoning_block.and_then(|r| str_at(r, &["default_effort"])),
            thinking_modes: Vec::new(),
        },
        supported_parameters,
    }
}

/// Effort levels low to high where we know the words, unknown ones after them in name order,
/// so the record reads the same whatever order a provider's JSON object happens to keep.
fn order_efforts(mut levels: Vec<String>) -> Vec<String> {
    const KNOWN: [&str; 7] = ["minimal", "none", "low", "medium", "high", "xhigh", "max"];
    levels.sort_by_key(|level| {
        (
            KNOWN.iter().position(|k| k == level).unwrap_or(KNOWN.len()),
            level.clone(),
        )
    });
    levels
}

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, key| v.get(key))
}

fn u32_at(value: &Value, path: &[&str]) -> Option<u32> {
    at(value, path)?.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    at(value, path)?.as_bool()
}

fn str_at(value: &Value, path: &[&str]) -> Option<String> {
    at(value, path)?.as_str().map(str::to_string)
}

fn strings_at(value: &Value, path: &[&str]) -> Vec<String> {
    at(value, path)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

// ── Settings › AI ──────────────────────────────────────────────────────────────

/// Where the window Settings › AI shows came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AiModelLimitsSource {
    /// The provider's own model list.
    Provider,
    /// Cmdr's built-in table: a verified floor that may have aged.
    BuiltInTable,
}

/// What Settings › AI shows under the model picker.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModelLimits {
    /// `None` when nothing knows the window, so the UI shows nothing rather than a guess.
    pub context_window_tokens: Option<u32>,
    /// Only ever the provider's figure: the table doesn't guess it.
    pub max_output_tokens: Option<u32>,
    pub source: Option<AiModelLimitsSource>,
}

/// The window and longest reply of `model` on the provider being set up, for Settings › AI.
/// Reads the cache, falling back to the budget's family table; a missing or stale catalog
/// starts a gated background refresh, and `AiModelCapabilitiesChanged` says when to ask again.
#[tauri::command]
#[specta::specta]
pub fn get_ai_model_limits(app: AppHandle, provider_id: String, base_url: String, model: String) -> AiModelLimits {
    refresh(&app, &provider_id, &base_url, Refresh::IfStale);
    let reported = lookup(&provider_id, &base_url, &model);
    limits_for(&model, reported.as_ref())
}

fn limits_for(model: &str, reported: Option<&ModelCapabilities>) -> AiModelLimits {
    use crate::agent::chat::budget::{WindowSource, known_window};
    let reported_window = reported.and_then(|c| c.context_window_tokens);
    let provider = crate::agent::chat::session::cloud_provider_tag(model);
    let window = known_window(provider, model, 0, reported_window.map(|t| t as usize));
    AiModelLimits {
        context_window_tokens: window.and_then(|w| u32::try_from(w.tokens).ok()),
        max_output_tokens: reported.and_then(|c| c.max_output_tokens),
        source: window.map(|w| match w.source {
            WindowSource::Provider => AiModelLimitsSource::Provider,
            WindowSource::FamilyTable | WindowSource::LocalServer => AiModelLimitsSource::BuiltInTable,
        }),
    }
}

#[cfg(test)]
#[path = "model_capabilities_test.rs"]
mod tests;
