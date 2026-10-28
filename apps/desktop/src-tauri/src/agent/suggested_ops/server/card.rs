//! What a server card shows: the fields the click saves, an edit's old values beside the new
//! ones, and the facts Cmdr reads NOW (an address it hasn't saved before, how many shares an SMB
//! host's edit touches).
//!
//! Pure over the stored proposal and a [`SavedServers`] snapshot. The fields are Cmdr's facts
//! about the save, so the dialog shows them unlabelled as the AI's; only the rationale is the
//! AI's words.

use serde::Serialize;

use super::saved::SavedServers;
use crate::agent::store::proposals::{ServerDraft, ServerProposal};
use crate::agent::types::DraftProtocol;

/// One field a server card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ServerField {
    /// The name the servers list shows (an SMB host's, an S3 account's, an NFS server's).
    Name,
    /// The host, or WebDAV's URL.
    Address,
    Port,
    /// The account it signs in as: SFTP, WebDAV, or SMB's username, or the S3 account's name.
    Account,
    /// SMB: the share saved under the host.
    Share,
    /// SFTP and WebDAV: the remote root.
    Folder,
    /// NFS: the export.
    Export,
    /// S3: the bucket.
    Bucket,
    /// Where a pane opens once connected.
    StartFolder,
    /// NFS: the user and group ids every request acts as.
    Uid,
    Gid,
}

/// One field's row on the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerFieldView {
    pub field: ServerField,
    /// What the click saves. `None` when an edit clears it (a start folder back to the root).
    pub value: Option<String>,
    /// Whether an edit changes this field. Always `false` on an add.
    pub changed: bool,
    /// The saved value an edit replaces, when it changes one that had a value.
    pub previous: Option<String>,
}

/// A server card's whole content, beside the group's own header and rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerProposalView {
    pub protocol: DraftProtocol,
    /// In the order the card lists them. A field neither side has is left out.
    pub fields: Vec<ServerFieldView>,
    /// An add at a host no saved server of any protocol is at. ❗ A fact Cmdr read now, so an
    /// address the AI calls "your NAS" that Cmdr has never saved says so.
    pub address_is_new: bool,
    /// An SMB edit: how many saved shares on that host it changes, counted NOW (what the click
    /// changes, not what the AI counted). `None` for every other card, and for a host that's
    /// gone (approving it then refuses).
    pub shares_affected: Option<u32>,
}

/// The card for a stored server proposal, against what's saved now.
pub(crate) fn server_card(proposal: &ServerProposal, saved: &SavedServers) -> ServerProposalView {
    let draft = &proposal.draft;
    let new = field_values(draft, saved);
    let fields = match &proposal.edit {
        None => new
            .into_iter()
            .filter_map(|(field, value)| {
                value.map(|value| ServerFieldView {
                    field,
                    value: Some(value),
                    changed: false,
                    previous: None,
                })
            })
            .collect(),
        Some(edit) => new
            .into_iter()
            .zip(field_values(&edit.previous, saved))
            .filter(|((_, value), (_, previous))| value.is_some() || previous.is_some())
            .map(|((field, value), (_, previous))| {
                let changed = value != previous;
                ServerFieldView {
                    field,
                    value,
                    changed,
                    previous: if changed { previous } else { None },
                }
            })
            .collect(),
    };
    let shares_affected = match (&proposal.edit, draft.protocol) {
        (Some(edit), DraftProtocol::Smb) => saved
            .place(&edit.editing)
            .map(|(row, _)| u32::try_from(row.places.len()).unwrap_or(u32::MAX)),
        _ => None,
    };
    ServerProposalView {
        protocol: draft.protocol,
        fields,
        address_is_new: proposal.edit.is_none() && !saved.knows_host_of(draft),
        shares_affected,
    }
}

/// Every field a card can show, in its order, with the draft's value for each.
fn field_values(draft: &ServerDraft, saved: &SavedServers) -> [(ServerField, Option<String>); 11] {
    let account = match draft.protocol {
        // The account the servers list names, else its id.
        DraftProtocol::S3 => draft
            .account_id
            .as_ref()
            .map(|id| saved.row(id).map_or_else(|| id.clone(), |row| row.name.clone())),
        DraftProtocol::Smb | DraftProtocol::Sftp | DraftProtocol::Webdav | DraftProtocol::Nfs => draft.username.clone(),
    };
    let number = |value: Option<u32>| value.map(|value| value.to_string());
    [
        (ServerField::Name, draft.display_name.clone()),
        (ServerField::Address, draft.host.clone().or_else(|| draft.url.clone())),
        (ServerField::Port, draft.port.map(|port| port.to_string())),
        (ServerField::Account, account),
        (ServerField::Share, draft.share.clone()),
        (ServerField::Folder, draft.remote_root.clone()),
        (ServerField::Export, draft.export.clone()),
        (ServerField::Bucket, draft.bucket.clone()),
        (ServerField::StartFolder, draft.start_folder.clone()),
        (ServerField::Uid, number(draft.uid)),
        (ServerField::Gid, number(draft.gid)),
    ]
}
