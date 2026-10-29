//! The plain shapes a server group stores: the draft, an edit's target, and the stored whole.
//!
//! Their own module, depending on nothing else in the spine, so the read side (`read.rs`) and
//! the write and claim side (`server.rs`) both name them without depending on each other.

use serde::{Deserialize, Serialize};

use crate::agent::types::{DraftProtocol, ServerSaveNote};

/// A server as a proposal describes it: the spine's own plain type, mapped to the saved-server
/// shapes a layer up.
///
/// ❌ No field can hold a secret: no password, key, key file, agent, or auto-reconnect.
/// `deny_unknown_fields` keeps it that way on the way back in, so a stored draft carrying any
/// other key reads as undecodable rather than as a draft with the extra quietly dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerDraft {
    pub protocol: DraftProtocol,
    /// The name the servers list shows. For SMB, the host's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// SMB: the share row saved under the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// SFTP: the remote root; WebDAV: the collection under the URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_root: Option<String>,
    /// WebDAV's URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// S3: the bucket a place opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    /// S3: the saved account the bucket place sits under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// NFS: the export path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    /// The folder a pane opens in once connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_folder: Option<String>,
    /// SMB edit: how many saved shares on the host the change touches, as the proposal counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shares_affected: Option<u32>,
}

impl ServerDraft {
    /// A draft of `protocol` with every other field empty.
    pub fn new(protocol: DraftProtocol) -> Self {
        Self {
            protocol,
            display_name: None,
            host: None,
            port: None,
            username: None,
            share: None,
            remote_root: None,
            url: None,
            bucket: None,
            account_id: None,
            export: None,
            uid: None,
            gid: None,
            start_folder: None,
            shares_affected: None,
        }
    }
}

/// What an edit changes: which saved place, and how that place looked when it was proposed.
/// One value, so an edit can't be stored without the `previous` its diff is checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEdit {
    /// The saved place's id, as `list_volumes` shows it.
    pub editing: String,
    /// The place's editable fields and address as the proposal read them.
    pub previous: ServerDraft,
}

/// A stored server group's proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerProposal {
    pub draft: ServerDraft,
    /// `None` for an add.
    pub edit: Option<ServerEdit>,
    /// What a partial save left undone, once one completed.
    pub note: Option<ServerSaveNote>,
}
