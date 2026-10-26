//! The specta-typed shapes the query commands return ([`ConversationDetailView`],
//! [`AttachmentRef`], the send refusal) and the pure mappings that produce them. Everything here
//! DROPS backend-only material — no reasoning blob, no provider state, no raw tool-result content
//! ever crosses.
//!
//! One stored row's display projection ([`MessageView`]) lives one level down, in
//! `agent::chat::message_view`, because the live stream carries it too; it is re-exported here.
//! A turn's live progress is one conversation-keyed event, `agent::chat::stream`.

use serde::{Deserialize, Serialize};

use crate::agent::chat::context::{AttachmentKind, EnvelopeAttachment};
pub use crate::agent::chat::message_view::{MessageBlock, MessageRoleView, MessageView};
use crate::agent::chat::stream::AgentErrorKindView;
use crate::agent::store::ConversationRow;

/// Why a send never started.
///
/// Returned rather than streamed, because every one of these is decided before a turn
/// exists and some before a conversation id does, leaving no thread to key an event on. It
/// carries the same typed kinds a mid-turn failure does, so the rail renders one set of
/// honest copy either way.
///
/// **Build one through [`AskCmdrSendRefusal::of`], [`AskCmdrSendRefusal::gated`], or
/// [`AskCmdrSendRefusal::detailed`], never
/// as a struct literal.** Both constructors report the anonymous `ask_cmdr_turn` refusal, and
/// these gates are the only account of the funnel's top: a send refused here never reaches
/// `run_turn`, so a literal that skipped the report would make "AI is off" and "nobody opened
/// the rail" the same empty chart. The fields stay public only because `specta` reads them.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrSendRefusal {
    pub kind: AgentErrorKindView,
    /// The source problem's own wording, when there is one worth showing (a store that
    /// wouldn't open). Display only: the frontend branches on `kind`, never on this.
    pub detail: Option<String>,
    /// Which rule of the organization's policy refused, exactly when `kind` is
    /// `managedByOrganization`, so the rail words that rule.
    pub managed: Option<crate::managed_policy::ManagedAiRefusal>,
}

impl AskCmdrSendRefusal {
    /// A refusal the kind says everything about.
    pub(super) fn of(kind: AgentErrorKindView) -> Self {
        crate::agent::chat::runtime::send_refused(kind);
        Self {
            kind,
            detail: None,
            managed: None,
        }
    }

    /// The send gate's refusal, carrying the organization's reason when the policy refused.
    pub(super) fn gated(gate: crate::agent::chat::session::SendGateRefusal) -> Self {
        crate::agent::chat::runtime::send_refused(gate.view());
        Self {
            kind: gate.view(),
            detail: None,
            managed: gate.managed(),
        }
    }

    /// A refusal carrying the source problem's own wording for display.
    pub(super) fn detailed(kind: AgentErrorKindView, detail: String) -> Self {
        crate::agent::chat::runtime::send_refused(kind);
        Self {
            kind,
            detail: Some(detail),
            managed: None,
        }
    }
}

/// What the chat-memory setting needs to warn honestly: the model the next turn would use,
/// and the context window we believe it has.
///
/// The window knowledge stays in `agent::chat::budget` (one table, not two), and the COMPARISON
/// stays in the settings UI: the user's pick has to warn the moment it's chosen, and the stored
/// value the backend reads lands up to half a second later.
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelWindowView {
    /// The model Ask Cmdr would send to right now. Empty when AI is off.
    pub model: String,
    /// That model's window in tokens: the local server's configured window, else the family
    /// table. `None` when nothing here knows it, so the UI stays quiet instead of guessing.
    pub known_window_tokens: Option<u32>,
}

/// A conversation header plus a page of its display messages, and the total count so a
/// paged UI knows whether more exist.
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetailView {
    pub conversation: ConversationRow,
    pub messages: Vec<MessageView>,
    pub total_messages: u32,
    /// What the thread's last measured turn spent, and the budget it spent it against, so a
    /// reopened thread shows its real gauge instead of an empty one. `None` until a turn has
    /// finished: read as "not measured yet", never as zero usage.
    pub last_context_usage: Option<ContextUsageView>,
    /// The running turn's seq high-water mark, when a turn runs in this thread right now: rows
    /// after it are that turn's, still being written, so a result-less call among them is running
    /// (every other result-less call is settled). `None` when nothing runs.
    pub turn_in_flight_since_seq: Option<i64>,
}

/// A thread's last measured context usage, on the wire. Both figures are `chars/4` estimates.
#[derive(Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageView {
    pub estimated_tokens: u32,
    pub budget_tokens: u32,
}

// ── Attachments (by reference; path + kind, never contents) ─────────────────────

/// Whether an attachment references a file or a folder, on the wire.
#[derive(Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKindView {
    File,
    Folder,
}

/// A file/folder the user attached by reference for a turn (dragged onto the composer,
/// or "ask about selection"). Structurally path + kind only — the read-only privacy
/// line means no tool ever reads its contents. Both directions: an input to
/// [`ask_cmdr_send_message`](super::chat::ask_cmdr_send_message), and the output of the two attachment-resolving commands.
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    pub path: String,
    pub kind: AttachmentKindView,
}

impl AttachmentRef {
    pub(super) fn to_envelope(&self) -> EnvelopeAttachment {
        EnvelopeAttachment {
            path: self.path.clone(),
            kind: match self.kind {
                AttachmentKindView::File => AttachmentKind::File,
                AttachmentKindView::Folder => AttachmentKind::Folder,
            },
        }
    }
}

#[cfg(test)]
mod tests;
