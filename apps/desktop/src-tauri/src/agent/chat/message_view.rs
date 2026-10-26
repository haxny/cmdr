//! The rail's display projection of one stored row ([`MessageView`]), and the pure mapping that
//! builds it. Everything here DROPS backend-only material: no reasoning blob, no provider state,
//! no raw tool-result content ever crosses.
//!
//! Below `commands/` because the live stream carries a view too: a persisted user row rides
//! `UserPersisted` as a [`MessageView`], so a rail that reloaded before the row landed can show
//! it, and a wake (which may not import upward) streams through the same transport.

use serde::Serialize;
use serde_json::Value;

use crate::agent::llm::types::{AgentPart, AgentRole};
use crate::agent::store::{self, StoredMessage};
use crate::agent::types::ProposalDecision;

/// A message's role, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageRoleView {
    System,
    User,
    Assistant,
    Tool,
    /// A UI-facing timeline entry (a model change), not transcript content.
    Event,
}

impl From<AgentRole> for MessageRoleView {
    fn from(role: AgentRole) -> Self {
        match role {
            AgentRole::System => Self::System,
            AgentRole::User => Self::User,
            AgentRole::Assistant => Self::Assistant,
            AgentRole::Tool => Self::Tool,
        }
    }
}

/// One display block of a message. A projection of the stored [`AgentPart`]s that DROPS
/// the reasoning part entirely (the opaque provider blob is backend-only and never
/// crosses IPC — the store's `content_blocks` invariant).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MessageBlock {
    /// Assistant or user prose (rendered markdown-lite, entity-escaped first).
    Text { text: String },
    /// A tool the model invoked. `arguments` is the raw JSON call arguments as a string
    /// (the frontend `JSON.parse`s it to build a localized "looked at X" label); both
    /// `tool` and any filesystem-derived args render as escaped plain text, never `{@html}`.
    ToolCall {
        call_id: String,
        tool: String,
        arguments: String,
    },
    /// A tool result, reduced to its status (`ok`/`elided`) — the raw content stays
    /// backend-only.
    ToolResult { call_id: String, ok: bool, elided: bool },
    /// The conversation's effective model changed between turns; `model` is the new name.
    /// Rendered as a small centered timeline line, escaped plain text (never `{@html}`).
    ModelChanged { model: String },
    /// How much of the conversation each message carries changed between turns, so the
    /// replies after this line saw a different amount of the chat. Rendered as a small
    /// centered timeline line beside the model one.
    ///
    /// ⚠️ **A number, never a sentence.** The row outlives every locale pass, so the rail
    /// formats the count in the user's own language and nothing English is frozen in
    /// `main.db`.
    ChatMemoryChanged { chat_memory_tokens: usize },
    /// What a wake noticed, which is the first message of every thread the agent opened for
    /// itself.
    ///
    /// ⚠️ **Numbers and paths, never a sentence.** The digest the model reads is rendered
    /// English (`agent/wake/compact.rs`) and it is persisted for as long as the thread lives,
    /// so shipping it as prose would freeze one locale's copy in `main.db` where no later
    /// locale pass could reach it. The rail says these counts in the user's own language and
    /// renders them collapsed.
    WakeDigest {
        folders: Vec<WakeDigestFolderView>,
        rollups: Vec<WakeDigestRollupView>,
    },
    /// What the user did with the proposals this thread made: one entry per answered group.
    ///
    /// Two rows reach the rail as this block, and deliberately as the SAME one. An `event`-role
    /// row carries a single decision as it happens, for the user's eyes; a `user`-role row
    /// carries a whole sweep's worth, because it is what opened the follow-up turn the agent
    /// learns from. One shape, one renderer, one set of strings.
    ///
    /// ⚠️ **Verbs, counts, and the group's own display text, never a sentence.** The English
    /// the model reads (`ProposalDecision::render`) is a prompt and stays one.
    ProposalDecisions { decisions: Vec<ProposalDecision> },
}

/// One folder a wake's digest named outright, and what happened in it. Every count is a
/// number: the rail owns every word around them.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakeDigestFolderView {
    /// An absolute path, rendered as escaped plain text (never `{@html}`).
    pub folder: String,
    pub created: u32,
    pub modified: u32,
    pub removed: u32,
    pub renamed: u32,
}

/// The folders a wake's digest did not have room to name, summarized under a shared
/// ancestor. Present so the collapsed block can admit how much it is not showing.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakeDigestRollupView {
    /// An absolute path, rendered as escaped plain text (never `{@html}`).
    pub ancestor: String,
    pub folders: u32,
    pub changes: u64,
}

/// A message as the rail displays it: id/seq/role, its display blocks, and token counts.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: i64,
    pub seq: i64,
    pub role: MessageRoleView,
    pub blocks: Vec<MessageBlock>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub created_at: i64,
}

/// True when a persisted tool result is a real answer rather than a refusal or a handler
/// problem. Reads OUR OWN typed result keys (`available` / `problem`), never external
/// wording — matching the runtime's `dispatch_ok`.
fn tool_result_ok(content: &Value) -> bool {
    let refused = content.get("available") == Some(&Value::Bool(false));
    let problem = content.get("problem").is_some();
    !(refused || problem)
}

/// Project one stored message into its display form, dropping reasoning parts. Event
/// rows project to `role: Event` with their single typed block.
pub fn to_message_view(message: StoredMessage) -> MessageView {
    let (role, blocks): (MessageRoleView, Vec<MessageBlock>) = match message.content {
        store::StoredContent::Message { role, parts } => {
            let blocks = parts
                .into_iter()
                .filter_map(|part| match part {
                    AgentPart::Text(text) => Some(MessageBlock::Text { text }),
                    AgentPart::ToolCall(call) => Some(MessageBlock::ToolCall {
                        call_id: call.call_id,
                        tool: call.tool.as_wire_name().to_string(),
                        arguments: call.arguments.to_string(),
                    }),
                    AgentPart::ToolResult(result) => Some(MessageBlock::ToolResult {
                        ok: tool_result_ok(&result.content),
                        call_id: result.call_id,
                        elided: result.elided,
                    }),
                    AgentPart::Reasoning(_) => None,
                    AgentPart::WakeDigest(digest) => Some(MessageBlock::WakeDigest {
                        folders: digest
                            .folders
                            .into_iter()
                            .map(|folder| WakeDigestFolderView {
                                folder: folder.folder,
                                created: folder.created,
                                modified: folder.modified,
                                removed: folder.removed,
                                renamed: folder.renamed,
                            })
                            .collect(),
                        rollups: digest
                            .rollups
                            .into_iter()
                            .map(|rollup| WakeDigestRollupView {
                                ancestor: rollup.ancestor,
                                folders: rollup.folders,
                                changes: rollup.changes,
                            })
                            .collect(),
                    }),
                    AgentPart::ProposalOutcomes(outcomes) => Some(MessageBlock::ProposalDecisions {
                        decisions: outcomes.decisions,
                    }),
                })
                .collect();
            (role.into(), blocks)
        }
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { model }) => {
            (MessageRoleView::Event, vec![MessageBlock::ModelChanged { model }])
        }
        store::StoredContent::Event(store::ConversationEvent::ChatMemoryChanged { chat_memory_tokens }) => (
            MessageRoleView::Event,
            vec![MessageBlock::ChatMemoryChanged { chat_memory_tokens }],
        ),
        // One decision, in the same block the follow-up turn's opener uses: the rail draws one
        // line per decision either way, and a second shape would be a second renderer for the
        // same sentence.
        store::StoredContent::Event(store::ConversationEvent::ProposalDecided { decision }) => (
            MessageRoleView::Event,
            vec![MessageBlock::ProposalDecisions {
                decisions: vec![decision],
            }],
        ),
        // A turn's stats ride its last assistant view, not a line of their own; and a kind a
        // newer build wrote has nothing to draw. Both stay an empty event view, so the row's seq
        // still holds its place in the timeline.
        store::StoredContent::Event(
            store::ConversationEvent::TurnFinished { .. } | store::ConversationEvent::Unknown,
        ) => (MessageRoleView::Event, Vec::new()),
    };
    MessageView {
        id: message.id,
        seq: message.seq,
        role,
        blocks,
        prompt_tokens: message.prompt_tokens,
        completion_tokens: message.completion_tokens,
        created_at: message.created_at,
    }
}

#[cfg(test)]
mod tests;
