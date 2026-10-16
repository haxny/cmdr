//! The write engine's engine-wide facts, as this app hears them.
//!
//! The engine reports facts through `EngineEvents`; [`TauriEngineEvents`] is
//! this app's answer and owns the policy on each: which ones become a Tauri
//! event, which feed indexing's "a transfer trumps me" gauge, and which become a
//! PostHog capture under which name.

use serde_json::{Value, json};
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::file_system::write_operations::host::{
    EngineEvents, InstantMutationFact, InstantOp, InstantOutcome, InstantTarget,
};
use crate::file_system::write_operations::{MoveLeftoversKeptEvent, OperationsChanged, VolumesBusyChanged};

/// Emits through the app handle when there is one; a test binary has none and
/// still feeds the gauge and the analytics client, which work without an app.
pub(super) struct TauriEngineEvents {
    app: Option<AppHandle>,
}

impl TauriEngineEvents {
    pub(super) fn new(app: Option<AppHandle>) -> Self {
        Self { app }
    }
}

impl EngineEvents for TauriEngineEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = snapshot.emit(app) {
            log::warn!(target: "op_manager", "failed to emit operations-changed: {e}");
        }
    }

    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = (VolumesBusyChanged { volume_ids }).emit(app) {
            crate::log_error!(target: "eject", "Failed to emit volumes-busy-changed: {}", e);
        }
    }

    fn transfer_started(&self, volume_ids: &[String]) {
        crate::priority::transfers::note_transfer_started(volume_ids);
    }

    fn transfer_finished(&self, volume_ids: &[String]) {
        crate::priority::transfers::note_transfer_finished(volume_ids);
    }

    fn staging_leftovers_kept(&self, notice: MoveLeftoversKeptEvent) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = notice.emit(app) {
            log::warn!(target: "copy", "couldn't tell the frontend about the kept staging folder: {e}");
        }
    }

    fn instant_mutation_settled(&self, fact: InstantMutationFact) {
        let props = instant_props(fact);
        // The event name is a LITERAL in each arm, never a variable holding one: the
        // `analytics-event-catalog` check reads emitters by matching `capture("…")`, so
        // a name that arrives as a `&str` would be invisible to it and could drift from
        // the catalog forever. Worth the duplicated call. The two creates get separate
        // names because "do people make folders?" and "do people make files?" are
        // different questions with very different expected answers.
        match fact.op {
            InstantOp::Rename => crate::analytics::events::capture("rename_used", props),
            InstantOp::CreateFolder => crate::analytics::events::capture("folder_created", props),
            InstantOp::CreateFile => crate::analytics::events::capture("file_created", props),
        }
    }
}

/// The props every instant-op event carries, built once so they can't drift:
/// who asked (the person, the agent, or an AI client), where it landed, and
/// whether it worked. Never a name, a path, or the reason it failed.
fn instant_props(fact: InstantMutationFact) -> Value {
    let target = match fact.target {
        InstantTarget::Volume => "volume",
        InstantTarget::Archive => "archive",
    };
    let outcome = match fact.outcome {
        InstantOutcome::Done => "done",
        InstantOutcome::Failed => "failed",
    };
    json!({
        "initiator": fact.initiator.as_token(),
        "target": target,
        "outcome": outcome,
    })
}

#[cfg(test)]
mod tests {
    use cmdr_operation_log::types::Initiator;

    use super::*;

    /// The props are what the dashboards query on, so their keys and tokens are
    /// pinned word for word.
    #[test]
    fn instant_props_carry_the_initiator_target_and_outcome_tokens() {
        let fact = InstantMutationFact {
            op: InstantOp::Rename,
            initiator: Initiator::User,
            target: InstantTarget::Archive,
            outcome: InstantOutcome::Failed,
        };
        assert_eq!(
            instant_props(fact),
            json!({ "initiator": Initiator::User.as_token(), "target": "archive", "outcome": "failed" })
        );
        let landed = InstantMutationFact {
            target: InstantTarget::Volume,
            outcome: InstantOutcome::Done,
            ..fact
        };
        assert_eq!(
            instant_props(landed),
            json!({ "initiator": Initiator::User.as_token(), "target": "volume", "outcome": "done" })
        );
    }
}
