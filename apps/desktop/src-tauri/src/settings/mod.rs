//! Settings module: loads settings from tauri-plugin-store's JSON file at startup.

mod ai_gates;
pub mod loader;
mod restricted_shortcuts;

#[cfg(test)]
mod plugin_store_tests;

pub use ai_gates::{
    AskCmdrSwitch, load_ask_cmdr_consent_revoke_pending, load_ask_cmdr_switch, load_cloud_consent_revoke_pending,
};
pub use loader::{
    FullDiskAccessChoice, RestrictedWindowSettings, early_load_global_go_to_latest_shortcut,
    early_load_max_log_storage_mb, early_load_verbose_logging, load_ai_reasoning_efforts,
    load_ask_cmdr_chat_memory_size, load_ask_cmdr_interactive_model, load_ask_cmdr_proactive,
    load_ask_cmdr_wake_delay_secs, load_operation_log_enabled, load_operation_log_retention_limits,
    load_restricted_window_settings, load_security_log_enabled, load_settings,
};
pub use restricted_shortcuts::load_custom_shortcuts;
