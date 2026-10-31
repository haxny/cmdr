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
//! is there (the table, on the first send after launch). A connection check stores what it already
//! fetched; a background refresh goes only to a trusted endpoint (see `Catalogs`). The cache is keyed by (provider id, base URL), the two things that decide which
//! catalog answers.
//!
//! **Egress**: a model-list call carries no user data, but it reaches the provider, so a
//! refresh asks the managed policy and cloud consent first, exactly like the connection check,
//! and goes only to a trusted endpoint (`Catalogs`). Ask Cmdr's lookup asks for one only while
//! `ai.provider` is `cloud`; Settings › AI reads the cache, and its connection check fills it.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Every catalog this run knows, and which endpoints may be fetched from in the background.
///
/// ⚠️ **A background fetch goes only to a TRUSTED endpoint**: one a connection check got a model
/// list from (debounced, after the user stopped typing), or the one Cmdr started with (from
/// `settings.json`, so committed). Never to an endpoint seen only through a lookup: a custom
/// base URL reaches the backend on every keystroke, and `https://api.mycompany.co` (a prefix
/// of `…company.com`) is a real host a stranger could own. The fetch carries the stored key.
#[derive(Debug, Default)]
struct Catalogs {
    slots: HashMap<CatalogKey, CatalogSlot>,
    trusted: HashSet<CatalogKey>,
}

impl Catalogs {
    /// Whether to start a background fetch for `key` at `now`, marking it in flight if so.
    fn begin_refresh(&mut self, key: &CatalogKey, now: Instant) -> bool {
        if !self.trusted.contains(key) {
            return false;
        }
        let slot = self.slots.entry(key.clone()).or_default();
        if !slot.wants_refresh(now) {
            return false;
        }
        slot.in_flight = true;
        true
    }

    /// A fetch that failed: the retry waits [`RETRY_AFTER`].
    fn fail(&mut self, key: &CatalogKey, now: Instant) {
        if let Some(slot) = self.slots.get_mut(key) {
            slot.in_flight = false;
            slot.last_attempt = Some(now);
        }
    }

    /// Keep `catalog` for `key`, which is now trusted, and drop every other endpoint but the
    /// configured one, so a provider tried once in Settings doesn't stay in memory forever.
    fn store(&mut self, key: CatalogKey, catalog: ModelCatalog, now: Instant, configured: Option<&CatalogKey>) {
        let slot = self.slots.entry(key.clone()).or_default();
        slot.catalog = Some((catalog, now));
        slot.last_attempt = Some(now);
        slot.in_flight = false;
        self.trusted.insert(key.clone());
        let keep = |k: &CatalogKey| *k == key || Some(k) == configured;
        self.slots.retain(|k, _| keep(k));
        self.trusted.retain(|k| keep(k));
    }

    fn lookup(&self, key: &CatalogKey, model: &str) -> Option<&ModelCapabilities> {
        let (catalog, _) = self.slots.get(key)?.catalog.as_ref()?;
        find_model(catalog, model)
    }
}

static CATALOGS: Mutex<Option<Catalogs>> = Mutex::new(None);

/// Whether `configure_ai` has handed over the endpoint Cmdr started with.
static STARTUP_TRUSTED: AtomicBool = AtomicBool::new(false);

/// Tells the windows a catalog arrived, so Settings › AI re-reads the window it shows.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
pub struct AiModelCapabilitiesChanged;

fn configured_key() -> Option<CatalogKey> {
    let provider_id = super::state::get_cloud_provider_id();
    let (_, base_url, _) = super::state::get_cloud_config();
    (!provider_id.is_empty() && !base_url.is_empty()).then(|| CatalogKey::new(&provider_id, &base_url))
}

/// Keep what a model list answered, for (`provider_id`, `base_url`), and trust that endpoint
/// for later background refreshes. Called by the connection check and by a refresh.
pub(super) fn store<R: Runtime>(app: &AppHandle<R>, provider_id: &str, base_url: &str, catalog: ModelCatalog) {
    let configured = configured_key();
    CATALOGS
        .lock_ignore_poison()
        .get_or_insert_with(Catalogs::default)
        .store(
            CatalogKey::new(provider_id, base_url),
            catalog,
            Instant::now(),
            configured.as_ref(),
        );
    if let Err(e) = AiModelCapabilitiesChanged.emit(app) {
        log::debug!("couldn't announce new model capabilities: {e}");
    }
}

/// What the cache knows about `model` on (`provider_id`, `base_url`), with no I/O.
pub fn lookup(provider_id: &str, base_url: &str, model: &str) -> Option<ModelCapabilities> {
    CATALOGS
        .lock_ignore_poison()
        .as_ref()?
        .lookup(&CatalogKey::new(provider_id, base_url), model)
        .cloned()
}

/// What the provider reports for `model` on the configured cloud provider, refreshing in the
/// background when the cache is stale. `None` when AI isn't on Cloud or nothing was reported.
pub fn for_configured_cloud_model<R: Runtime>(app: &AppHandle<R>, model: &str) -> Option<ModelCapabilities> {
    if super::state::get_provider() != "cloud" {
        return None;
    }
    let provider_id = super::state::get_cloud_provider_id();
    let (_, base_url, _) = super::state::get_cloud_config();
    refresh(app, &provider_id, &base_url);
    lookup(&provider_id, &base_url, model)
}

/// `configure_ai` hands over the endpoint it just stored. The FIRST one of a run is what Cmdr
/// started with, read from `settings.json`, so it's committed and trusted; a later one may be a
/// base URL mid-typing and is trusted only once a connection check vouches for it.
pub fn configured<R: Runtime>(app: &AppHandle<R>, provider_id: &str, base_url: &str, cloud_on: bool) {
    let first = !STARTUP_TRUSTED.swap(true, Ordering::SeqCst);
    if provider_id.is_empty() || base_url.is_empty() {
        return;
    }
    if first {
        CATALOGS
            .lock_ignore_poison()
            .get_or_insert_with(Catalogs::default)
            .trusted
            .insert(CatalogKey::new(provider_id, base_url));
    }
    if cloud_on {
        refresh(app, provider_id, base_url);
    }
}

/// Start one background refresh of (`provider_id`, `base_url`)'s catalog if that endpoint is
/// trusted and its catalog missing or stale. Returns at once; the result lands in the cache and
/// announces itself.
pub fn refresh<R: Runtime>(app: &AppHandle<R>, provider_id: &str, base_url: &str) {
    if provider_id.is_empty() || base_url.is_empty() {
        return;
    }
    let key = CatalogKey::new(provider_id, base_url);
    let go = CATALOGS
        .lock_ignore_poison()
        .get_or_insert_with(Catalogs::default)
        .begin_refresh(&key, Instant::now());
    if !go {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match fetch_catalog(&app, &key).await {
            Some(catalog) => {
                log::debug!(
                    "model capabilities for {}: {} models reported",
                    key.provider_id,
                    catalog.len()
                );
                store(&app, &key.provider_id, &key.base_url, catalog);
            }
            None => {
                if let Some(catalogs) = CATALOGS.lock_ignore_poison().as_mut() {
                    catalogs.fail(&key, Instant::now());
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
    /// What the Effort picker may offer for this model (`agent::chat::effort::effort_options`).
    pub effort: AiEffortOptions,
}

/// One effort level the picker offers, cheapest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AiEffortLevel {
    Low,
    Medium,
    High,
    Max,
}

/// The Effort picker's menu for one model. The UI stores the pick in `ai.reasoningEfforts` as
/// `"recommended"` or a level's camelCase name; the backend maps a pick the model can't take to
/// the nearest one it can, so this menu is a courtesy, never the guard. There's no Off: thinking
/// stays on for users, and a stored `"off"` from an older build reads as Recommended.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiEffortOptions {
    /// Whether the model reasons: `false` hides the picker, `None` means nothing knows (the
    /// picker then offers Recommended alone).
    pub reasons: Option<bool>,
    pub levels: Vec<AiEffortLevel>,
    /// What Recommended amounts to, when anything knows; `None` while the provider decides
    /// per request.
    pub recommended: Option<AiEffortLevel>,
    /// Where the levels came from: the provider's list, or Cmdr's built-in table.
    pub source: Option<AiModelLimitsSource>,
}

/// The window and longest reply of `model` on the provider being set up, for Settings › AI.
/// Reads the cache, falling back to the budget's family table. A stale catalog for a TRUSTED
/// endpoint refreshes in the background; an endpoint being typed waits for the connection check.
/// `AiModelCapabilitiesChanged` says when to ask again.
#[tauri::command]
#[specta::specta]
pub fn get_ai_model_limits(app: AppHandle, provider_id: String, base_url: String, model: String) -> AiModelLimits {
    refresh(&app, &provider_id, &base_url);
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
        effort: effort_view(provider, model, reported),
    }
}

fn effort_view(
    provider: crate::agent::llm::types::ProviderTag,
    model: &str,
    reported: Option<&ModelCapabilities>,
) -> AiEffortOptions {
    use crate::agent::chat::effort::{EffortLevel, KnowledgeSource, effort_options};
    let level = |level: EffortLevel| match level {
        EffortLevel::Low => AiEffortLevel::Low,
        EffortLevel::Medium => AiEffortLevel::Medium,
        EffortLevel::High => AiEffortLevel::High,
        EffortLevel::Max => AiEffortLevel::Max,
    };
    let options = effort_options(provider, model, reported.map(|c| &c.reasoning));
    AiEffortOptions {
        reasons: options.reasons,
        levels: options.levels.into_iter().map(level).collect(),
        recommended: options.recommended.map(level),
        source: options.source.map(|source| match source {
            KnowledgeSource::Provider => AiModelLimitsSource::Provider,
            KnowledgeSource::BuiltIn => AiModelLimitsSource::BuiltInTable,
        }),
    }
}

#[cfg(test)]
#[path = "model_capabilities_test.rs"]
mod tests;
