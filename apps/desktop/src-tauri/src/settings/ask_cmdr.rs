//! The AI and Ask Cmdr settings the agent reads fresh from `settings.json`: per send (the
//! interactive model, the chat memory size, the reasoning efforts, the spend pause) or each time
//! the wake loop services a change (proactive, the wake delay). Read fresh, so a change applies to
//! the next message or wake with no restart and never moves a turn in flight.
//!
//! Each reader has a pure `parse_*` half so the parsing is tested without an app handle
//! (`loader_tests.rs`).

use std::fs;

/// The Ask Cmdr interactive-slot model override, read fresh from `settings.json` each
/// send (so a settings change takes effect on the next message, no restart).
///
/// Empty/absent ⇒ use the model the shared `ai/` provider is already configured with (the
/// v1 default, zero extra config). A non-empty value is a dedicated model id for Ask Cmdr,
/// layered OVER the shared `ai/` provider config (agent decision D43: two slots, interactive +
/// a later bulk slot). The bulk slot slots in beside this as its own additive key
/// (`askCmdr.bulkModel`), no migration. Only the model is slot-specific; provider on/off,
/// keys, and base URLs stay single-sourced in the `ai/` config (D49: extend, don't fork).
pub fn load_ask_cmdr_interactive_model<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let contents = fs::read_to_string(data_dir.join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("askCmdr.interactiveModel")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// The user's Ask Cmdr chat-memory size (`askCmdr.chatMemorySize`), read fresh from
/// `settings.json` each send, so a change applies to the next message with no restart and no
/// `settings-applier` case (the same read-fresh shape as
/// [`load_ask_cmdr_interactive_model`]).
///
/// `None` means "Automatic (recommended)": the budget module follows the model's window
/// instead. The stored value is one of a fixed preset list, so anything else (an absent key,
/// `auto`, a hand-edited string) reads as Automatic rather than as a number to sanitize.
pub fn load_ask_cmdr_chat_memory_size<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<usize> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let contents = fs::read_to_string(data_dir.join("settings.json")).ok()?;
    parse_ask_cmdr_chat_memory_size(&contents)
}

/// The parsing half of [`load_ask_cmdr_chat_memory_size`], pure so it can be tested without an
/// app handle. Anything that isn't a positive number of tokens reads as Automatic.
pub(super) fn parse_ask_cmdr_chat_memory_size(contents: &str) -> Option<usize> {
    let json: serde_json::Value = serde_json::from_str(contents).ok()?;
    json.get("askCmdr.chatMemorySize")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .filter(|tokens| *tokens > 0)
}

/// The default of `askCmdr.spendPause`, in micro-USD, mirrored from the frontend registry
/// (`definitions/ai.ts`): the store is sparse, so an untouched row reaches here as an absent key.
/// Why $2: `agent/chat/DETAILS.md` § Constants table.
pub const DEFAULT_ASK_CMDR_SPEND_PAUSE_MICROS: i64 = 2_000_000;

/// Where one Ask Cmdr answer pauses on spend and asks to keep going (`askCmdr.spendPause`), in
/// micro-USD of estimated spend, read fresh from `settings.json` each send like
/// [`load_ask_cmdr_chat_memory_size`], so a change applies to the next message. `None` is the
/// user's "Never".
pub fn load_ask_cmdr_spend_pause_micros<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<i64> {
    let contents = crate::config::resolved_app_data_dir(app)
        .ok()
        .and_then(|dir| fs::read_to_string(dir.join("settings.json")).ok());
    parse_ask_cmdr_spend_pause_micros(contents.as_deref())
}

/// The parsing half of [`load_ask_cmdr_spend_pause_micros`], pure so it can be tested without an
/// app handle. `"never"` is `None`; a positive dollar amount is that many micro-USD; anything
/// else (an absent file or key, a hand-edited string) reads as the default, since a pause the
/// user never chose to drop is the safer side to fall on.
pub(super) fn parse_ask_cmdr_spend_pause_micros(contents: Option<&str>) -> Option<i64> {
    let stored = contents
        .and_then(|contents| serde_json::from_str::<serde_json::Value>(contents).ok())
        .and_then(|json| {
            json.get("askCmdr.spendPause")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let Some(stored) = stored else {
        return Some(DEFAULT_ASK_CMDR_SPEND_PAUSE_MICROS);
    };
    if stored.trim() == "never" {
        return None;
    }
    let micros = stored
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|dollars| dollars.is_finite() && *dollars > 0.0)
        .map(|dollars| (dollars * 1_000_000.0).round() as i64);
    Some(micros.unwrap_or(DEFAULT_ASK_CMDR_SPEND_PAUSE_MICROS))
}

/// The per-model reasoning-effort choices (`ai.reasoningEfforts`), read fresh from
/// `settings.json` per send: the stored JSON object, or `"{}"` when absent. Parsing and the
/// key shape live with the resolution (`agent::chat::effort::choice_for`), which reads anything
/// unreadable as Recommended.
pub fn load_ai_reasoning_efforts<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    let contents = crate::config::resolved_app_data_dir(app)
        .ok()
        .and_then(|dir| fs::read_to_string(dir.join("settings.json")).ok());
    contents
        .as_deref()
        .and_then(parse_ai_reasoning_efforts)
        .unwrap_or_else(|| "{}".to_string())
}

/// The parsing half of [`load_ai_reasoning_efforts`], pure so it can be tested without an app.
pub(super) fn parse_ai_reasoning_efforts(contents: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(contents).ok()?;
    json.get("ai.reasoningEfforts")?.as_str().map(str::to_string)
}

/// Whether the user let Ask Cmdr start conversations on its own (`askCmdr.proactive`), read
/// fresh from `settings.json` so a change applies with no restart.
///
/// ⚠️ `None` means the key is ABSENT, not `false`. The store is sparse (it holds only what an
/// actor explicitly set), so the caller substitutes the registry default rather than reading
/// an untouched row as an opt-out. `agent::wake::WakeSettings` is that caller.
pub fn load_ask_cmdr_proactive<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<bool> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let contents = fs::read_to_string(data_dir.join("settings.json")).ok()?;
    parse_ask_cmdr_proactive(&contents)
}

/// The parsing half of [`load_ask_cmdr_proactive`], pure so it can be tested without an app
/// handle. Only a real JSON boolean counts; anything else is an unset key.
pub(super) fn parse_ask_cmdr_proactive(contents: &str) -> Option<bool> {
    let json: serde_json::Value = serde_json::from_str(contents).ok()?;
    json.get("askCmdr.proactive").and_then(|v| v.as_bool())
}

/// The user's wake cadence in seconds (`askCmdr.wakeDelay`), read fresh from `settings.json`
/// so a change applies with no restart.
///
/// The number is the HOT tier's delay, the one cadence value the slider moves; warm derives
/// from it. `None` for an absent or nonsensical value, so the caller substitutes the registry
/// default and clamps to the slider's track.
pub fn load_ask_cmdr_wake_delay_secs<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<u64> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let contents = fs::read_to_string(data_dir.join("settings.json")).ok()?;
    parse_ask_cmdr_wake_delay_secs(&contents)
}

/// The parsing half of [`load_ask_cmdr_wake_delay_secs`], pure so it can be tested without an
/// app handle. A zero or a negative is not a cadence anybody chose, so it reads as unset.
pub(super) fn parse_ask_cmdr_wake_delay_secs(contents: &str) -> Option<u64> {
    let json: serde_json::Value = serde_json::from_str(contents).ok()?;
    json.get("askCmdr.wakeDelay")
        .and_then(serde_json::Value::as_u64)
        .filter(|secs| *secs > 0)
}
