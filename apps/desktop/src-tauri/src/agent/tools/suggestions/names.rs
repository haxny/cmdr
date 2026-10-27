//! The names a proposal gives its files: a rename's, and a move's or copy's landing under a
//! new one.
//!
//! Every rule here is the write engine's own, asked ahead of time: a plain leaf
//! (`cmdr_fs::validation::validate_plain_leaf`), and for a move or copy the landing rules its
//! start enforces (`TransferLeaves::check_landings`: distinct ignoring case and Unicode
//! form, nothing landing on another source, no name into a source's own folder). ❌ Never a
//! second copy of either: a proposal that passes here and refuses at the start costs the user
//! an approval that does nothing. The one rule the engine doesn't hold is a phone's, so it
//! lives here alone. The rules and the why: `DETAILS.md` § Moving or copying under a new name.

use std::path::Path;

use cmdr_fs::validation::{PlainLeafRefusal, validate_plain_leaf};
use cmdr_operations::{LandingClash, TransferLeaves};

use super::input::GroupProblem;
use crate::agent::store::proposals::TransferOp;
use crate::mcp::resources::volumes::VolumeKind;

/// The most UTF-16 code units a phone takes in a name: a PTP string's 255 count its
/// terminator (`crates/cmdr-mtp/DETAILS.md` § Names a phone takes).
const PHONE_NAME_UNITS: usize = 254;

/// The characters Android's shared storage refuses in a name, FAT's reserved set.
const PHONE_RESERVED: [char; 8] = ['"', '*', ':', '<', '>', '?', '\\', '|'];

/// Which rule a proposed name breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameRule {
    /// One of [`validate_plain_leaf`]'s.
    Plain(PlainLeafRefusal),
    /// Past [`PHONE_NAME_UNITS`] on a phone.
    PhoneTooLong,
    /// One of [`PHONE_RESERVED`] on a phone.
    PhoneReserved(char),
}

impl NameRule {
    /// What's wrong with the name, as the end of a sentence the model reads.
    pub(super) fn reason(self) -> String {
        match self {
            NameRule::Plain(PlainLeafRefusal::Empty) => "it's empty".into(),
            NameRule::Plain(PlainLeafRefusal::EdgeWhitespace) => "it starts or ends with a space".into(),
            NameRule::Plain(PlainLeafRefusal::Separator) => {
                "it holds a /, so it's a path; give the bare name it lands as".into()
            }
            NameRule::Plain(PlainLeafRefusal::DotName) => "it's . or .., which name a folder".into(),
            NameRule::Plain(PlainLeafRefusal::TooLong { bytes }) => {
                format!("it's longer than the 255 bytes a file name may be ({bytes})")
            }
            NameRule::Plain(PlainLeafRefusal::Control(ch)) => {
                format!("it holds the control character U+{:04X}", u32::from(ch))
            }
            NameRule::Plain(PlainLeafRefusal::Invisible(ch)) => format!(
                "it holds the invisible character U+{:04X}, which would make it read as something else",
                u32::from(ch)
            ),
            NameRule::PhoneTooLong => {
                format!("it's longer than the {PHONE_NAME_UNITS} UTF-16 characters a phone takes in a name")
            }
            NameRule::PhoneReserved(ch) => format!("a phone refuses {ch} in a name"),
        }
    }
}

/// A rename's new name, or the refusal saying which rule it breaks.
pub(super) fn plain_name(name: &str) -> Result<(), GroupProblem> {
    validate_plain_leaf(name).map_err(|rule| not_a_bare_name(name, NameRule::Plain(rule)))
}

/// Normalize a move's or copy's names, then check them the way its start will. `ops` are
/// PLACED (re-addressed and rebased), `destination` is the placed folder, and
/// `destination_kind` its volume's kind.
///
/// A name equal to the source's own becomes `None` first, because "list every file, give a
/// file its current name to keep it" is the contract and the review should draw an arrow only
/// where a name changes. A group whose every name normalizes away is a plain move or copy and
/// gets no name checks, the same as one sent as `paths`.
pub(super) fn check_transfer_names(
    ops: &mut [TransferOp],
    destination: &str,
    destination_kind: Option<VolumeKind>,
) -> Result<(), GroupProblem> {
    for op in ops.iter_mut() {
        let own = Path::new(&op.source_path).file_name().and_then(|name| name.to_str());
        if op.new_name.as_deref() == own {
            op.new_name = None;
        }
    }
    if ops.iter().all(|op| op.new_name.is_none()) {
        return Ok(());
    }

    let items: Vec<(&Path, Option<&str>)> = ops
        .iter()
        .map(|op| (Path::new(&op.source_path), op.new_name.as_deref()))
        .collect();
    TransferLeaves::check_landings(&items, Path::new(destination)).map_err(|clash| match clash {
        LandingClash::NotPlain { leaf, refusal } => not_a_bare_name(&leaf, NameRule::Plain(refusal)),
        LandingClash::SameName { landing } => GroupProblem::DuplicateLeaf { name: leaf(&landing) },
        LandingClash::OntoASource { landing } => GroupProblem::ChainOverwrite {
            path: landing.display().to_string(),
        },
        LandingClash::AlreadyInDestination { .. } => GroupProblem::UseRename,
    })?;

    if matches!(destination_kind, Some(VolumeKind::Mtp | VolumeKind::Adb)) {
        for name in ops.iter().filter_map(|op| op.new_name.as_deref()) {
            phone_name(name)?;
        }
    }
    Ok(())
}

/// What a phone takes in a name beyond a plain leaf. Proposal-time only: nothing below
/// refuses these before the device does (`crates/cmdr-mtp/DETAILS.md` § Names a phone takes).
fn phone_name(name: &str) -> Result<(), GroupProblem> {
    if name.encode_utf16().count() > PHONE_NAME_UNITS {
        return Err(not_a_bare_name(name, NameRule::PhoneTooLong));
    }
    if let Some(ch) = name.chars().find(|ch| PHONE_RESERVED.contains(ch)) {
        return Err(not_a_bare_name(name, NameRule::PhoneReserved(ch)));
    }
    Ok(())
}

fn not_a_bare_name(name: &str, rule: NameRule) -> GroupProblem {
    GroupProblem::NotABareName {
        name: name.to_string(),
        rule,
    }
}

/// The last component of a landing, which is the name the model chose.
fn leaf(landing: &Path) -> String {
    landing
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
