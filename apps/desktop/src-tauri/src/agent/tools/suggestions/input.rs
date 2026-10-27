//! What `propose_suggestions` accepts, and the one place a call becomes something the store
//! can hold.
//!
//! The agent-facing contract lives here: a group names a verb, the target that verb binds,
//! and its sources — either an explicit path list or a **selector**, never both. Validation
//! is total and runs BEFORE anything is written, so a call with one bad group stages
//! nothing at all (`propose_rename_plan`'s rule, for the same reason: a partial proposal
//! reads as a whole one).
//!
//! Validation also PLACES every location a group names (`place.rs`), so what comes out is
//! where the group will really run: a re-addressed group already names the sibling it runs on.
//!
//! The planned shapes mirror [`GroupIntent`]'s pairing, so once validation is past, a trash
//! group with a destination or a rename whose names came from a pattern is unrepresentable
//! rather than checked again downstream.

use serde::Deserialize;

use super::place::{GroupPlacer, PlaceField};
use crate::agent::store::proposals::{GroupIntent, NewOp, NewRename, WritableDestination};
use crate::agent::suggested_ops::OpSelector;
use crate::agent::tools::read::expand_tilde;
use crate::agent::types::ProposalVerb;
use crate::location::Location;
use crate::location::miss::LocationMissResult;
use crate::location::resolve::{LocationInput, VolumeFacts};
use crate::location::same_share::Via;

/// How many groups one call may propose: a sweep is what the user reads in one sitting.
pub(super) const MAX_GROUPS: usize = 16;
/// How many paths one group may name explicitly. Past this a selector is the answer, and
/// the tool description says so — a list the model can't hold is one it starts inventing.
pub(super) const MAX_PATHS: usize = 200;

const SECONDS_PER_DAY: i64 = 86_400;

// ── What the model sends ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SweepInput {
    /// An existing sweep to extend or amend. Absent ⇒ a new sweep.
    pub sweep_id: Option<i64>,
    /// The agent's words for the sweep as a whole.
    pub rationale: Option<String>,
    pub groups: Vec<GroupInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GroupInput {
    /// A pending group to replace, inside the sweep that owns it.
    pub group_id: Option<i64>,
    pub verb: ProposalVerb,
    /// The shared destination directory (move / copy / extract), or the archive to write
    /// (compress). `volumeId` is optional: a bare path resolves to the live volume holding it.
    pub destination: Option<LocationInput>,
    /// Compress only: whether that archive already exists, which is what decides whether
    /// the group can be taken back.
    pub overwrites_existing: Option<bool>,
    /// Rename only: the folder every source shares.
    pub parent: Option<String>,
    /// The volume every source lives on. Required with `paths` / `renames`, refused with a
    /// selector (whose root already names one).
    pub source_volume_id: Option<String>,
    /// The group's title in the review dialog. Required with `paths` / `renames`, refused
    /// with a selector (whose pattern names the group).
    pub display_name: Option<String>,
    /// The agent's reason, shown labelled as the agent's words.
    pub rationale: Option<String>,
    pub paths: Option<Vec<String>>,
    pub renames: Option<Vec<RenameInput>>,
    pub selector: Option<SelectorInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RenameInput {
    pub path: String,
    /// A NAME, never a path: the executor refuses a row that would change the parent.
    pub new_name: String,
}

/// A selector as the model writes it: a subtree, a name glob, and predicates in units a
/// model states reliably (whole days ago, whole bytes) rather than epochs it would have to
/// compute.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SelectorInput {
    pub root: LocationInput,
    pub name_glob: Option<String>,
    pub min_size_bytes: Option<u64>,
    pub max_size_bytes: Option<u64>,
    /// Matches files last modified more than this many days ago.
    pub older_than_days: Option<u32>,
    /// Matches files last modified within this many days.
    pub newer_than_days: Option<u32>,
}

// ── What validation produces ──────────────────────────────────────────────────

/// A validated call: which sweep it lands in, and the groups it carries.
#[derive(Debug)]
pub(super) struct PlannedSweep {
    pub sweep_id: Option<i64>,
    pub rationale: Option<String>,
    pub groups: Vec<PlannedGroup>,
}

/// One validated, PLACED group: every location in it is where the resolver put it
/// (`place.rs`), re-addressed onto a live sibling when the volume asked isn't live. Its ops
/// are still a pattern when a selector produced it: resolution belongs to the write path,
/// which does it once and freezes the result.
#[derive(Debug)]
pub(super) struct PlannedGroup {
    /// The pending group this replaces, when it's an amendment.
    pub group_id: Option<i64>,
    pub rationale: Option<String>,
    pub ops: PlannedOps,
    /// The volume the model named, when the group was re-addressed onto a live sibling of it.
    pub via: Option<Via>,
}

/// The two op shapes, split the way the executors split them: rename carries its own
/// destinations, every other verb takes bare sources under a target the group binds.
#[derive(Debug)]
pub(super) enum PlannedOps {
    /// Rename: per-op names under a shared parent. ❌ A selector can never produce these —
    /// a pattern matches files, it can't decide what they should be called.
    Rename {
        parent: String,
        renames: Vec<NewRename>,
        naming: ExplicitNaming,
    },
    Sources {
        shape: SourceShape,
        sources: PlannedSources,
    },
}

/// What names a group the model listed by hand: the volume its sources share, and the title
/// the review dialog leads with. A selector group carries neither, because its pattern
/// answers both (`suggested_ops::selector_group`), which is why they live down here beside
/// the sources rather than on every group.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ExplicitNaming {
    pub source_volume_id: String,
    pub display_name: String,
}

impl ExplicitNaming {
    /// The same naming, on the volume its sources were placed on.
    fn on(self, source_volume_id: String) -> Self {
        Self {
            source_volume_id,
            ..self
        }
    }
}

/// A verb and the target it binds, with its sources still to come. The same pairing
/// [`GroupIntent`] encodes, held one step earlier so a selector's resolved ops drop in
/// without a second chance to pair them wrongly.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SourceShape {
    Move {
        destination: WritableDestination,
    },
    Copy {
        destination: WritableDestination,
    },
    Extract {
        destination: WritableDestination,
    },
    Compress {
        archive: WritableDestination,
        overwrites_existing: bool,
    },
    Trash,
    Delete,
}

/// Where a group's sources come from: named one by one, or described by a pattern the
/// backend resolves against the drive index.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PlannedSources {
    /// Sources named one by one. Their creation snapshot starts empty and is filled by one
    /// bounded stat each before the write (`snapshot.rs`).
    Paths {
        ops: Vec<NewOp>,
        naming: ExplicitNaming,
    },
    Selector(OpSelector),
}

impl SourceShape {
    /// Fill in the sources this shape was waiting for. Total: every arm pairs its verb with
    /// the target that verb's executor binds, so there is no wrong combination left to
    /// reject.
    pub(super) fn into_intent(self, sources: Vec<NewOp>) -> GroupIntent {
        match self {
            SourceShape::Move { destination } => GroupIntent::Move { destination, sources },
            SourceShape::Copy { destination } => GroupIntent::Copy { destination, sources },
            SourceShape::Extract { destination } => GroupIntent::Extract { destination, sources },
            SourceShape::Compress {
                archive,
                overwrites_existing,
            } => GroupIntent::Compress {
                archive,
                sources,
                overwrites_existing,
            },
            SourceShape::Trash => GroupIntent::Trash { sources },
            SourceShape::Delete => GroupIntent::Delete { sources },
        }
    }
}

// ── Refusals ──────────────────────────────────────────────────────────────────

/// Why a call staged nothing. Typed, and every group-level variant carries the group's
/// position in the call, because the model's only recovery is to send the whole call again
/// and it needs to know which group to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PlanRefusal {
    /// The JSON didn't parse into the tool's shape at all.
    Malformed,
    NoGroups,
    TooManyGroups {
        sent: usize,
    },
    /// A `groupId` with no `sweepId`: an amendment has to say which sweep it amends.
    GroupIdWithoutSweep {
        group: usize,
    },
    Group {
        group: usize,
        problem: GroupProblem,
    },
}

/// What one group got wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GroupProblem {
    /// Neither `paths`, `renames`, nor `selector`.
    NoSources,
    /// More than one of them: which one the user would be reviewing is a coin toss.
    AmbiguousSources,
    /// `renames` under a verb that isn't `rename`, or `rename` without them.
    RenamesVerbMismatch,
    /// A selector under `rename`.
    SelectorCantRename,
    /// A destination a ROUTE serves, which takes no writes: inside an archive, or inside
    /// a repo's virtual `.git` trees. Copy or move INTO a zip, and move OUT of one, plan
    /// from their own walk rather than the per-source engine, so a group written there
    /// could not be held to the sources the user reviewed; a snapshot has no file to
    /// write to at all. Refused HERE, where it costs the model a retry, rather than at
    /// execution where it would refuse after the user had already approved.
    DestinationTakesNoWrites,
    /// A field the verb's executor doesn't bind (a destination on a trash group, a parent
    /// on a move).
    UnboundField {
        field: &'static str,
    },
    /// A field the verb's executor requires.
    MissingField {
        field: &'static str,
    },
    /// `sourceVolumeId` / `displayName` sent alongside a selector, which supplies both.
    SelectorSuppliesField {
        field: &'static str,
    },
    EmptySources,
    TooManyPaths {
        sent: usize,
    },
    /// A path that is neither absolute nor a `scheme://` virtual path.
    RelativePath {
        path: String,
    },
    /// A rename destination that is a path rather than a bare name.
    NotABareName {
        name: String,
    },
    /// A size or age window nothing can satisfy.
    ImpossibleWindow,
    /// A place the resolver couldn't put on a live volume, nor re-address onto a live sibling.
    Location {
        field: PlaceField,
        miss: Box<LocationMissResult>,
    },
    /// Re-addressing would take the group onto two members of a share, or re-address one
    /// place while another names a member of that share directly: one `via` can't say it.
    MixedReaddress,
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Parse, validate, and place a whole call. `now` is unix seconds, injected so an age
/// predicate is deterministic in a test; `facts` is the call's one volume snapshot, which every
/// place in every group resolves against (`place.rs`).
pub(super) fn plan_sweep(
    params: &serde_json::Value,
    now: i64,
    facts: &VolumeFacts,
) -> Result<PlannedSweep, PlanRefusal> {
    let input: SweepInput = serde_json::from_value(params.clone()).map_err(|_| PlanRefusal::Malformed)?;
    if input.groups.is_empty() {
        return Err(PlanRefusal::NoGroups);
    }
    if input.groups.len() > MAX_GROUPS {
        return Err(PlanRefusal::TooManyGroups {
            sent: input.groups.len(),
        });
    }
    let mut groups = Vec::with_capacity(input.groups.len());
    for (index, group) in input.groups.into_iter().enumerate() {
        if group.group_id.is_some() && input.sweep_id.is_none() {
            return Err(PlanRefusal::GroupIdWithoutSweep { group: index });
        }
        groups.push(plan_group(group, now, facts).map_err(|problem| PlanRefusal::Group { group: index, problem })?);
    }
    Ok(PlannedSweep {
        sweep_id: input.sweep_id,
        rationale: input.rationale,
        groups,
    })
}

fn plan_group(group: GroupInput, now: i64, facts: &VolumeFacts) -> Result<PlannedGroup, GroupProblem> {
    let is_rename = group.verb == ProposalVerb::Rename;
    let sources_given = [group.paths.is_some(), group.renames.is_some(), group.selector.is_some()]
        .into_iter()
        .filter(|given| *given)
        .count();
    match sources_given {
        0 => return Err(GroupProblem::NoSources),
        1 => {}
        _ => return Err(GroupProblem::AmbiguousSources),
    }
    if group.renames.is_some() != is_rename {
        return Err(GroupProblem::RenamesVerbMismatch);
    }
    if is_rename && group.selector.is_some() {
        return Err(GroupProblem::SelectorCantRename);
    }
    if is_rename {
        if group.destination.is_some() {
            return Err(GroupProblem::UnboundField { field: "destination" });
        }
        if group.overwrites_existing.is_some() {
            return Err(GroupProblem::UnboundField {
                field: "overwritesExisting",
            });
        }
    } else if group.parent.is_some() {
        return Err(GroupProblem::UnboundField { field: "parent" });
    }

    // A selector names its own volume (its root's) and its own title (its pattern), so
    // either field sent alongside it would be a second, drifting answer to a settled
    // question.
    if group.selector.is_some() {
        if group.source_volume_id.is_some() {
            return Err(GroupProblem::SelectorSuppliesField {
                field: "sourceVolumeId",
            });
        }
        if group.display_name.is_some() {
            return Err(GroupProblem::SelectorSuppliesField { field: "displayName" });
        }
    }

    // The shape checks come first, so a group that's wrong on its face says so before any
    // place in it is looked up. Every place then resolves, and what's stored is where it landed.
    let mut placer = GroupPlacer::new(facts);
    let ops = match (group.renames, group.paths, group.selector) {
        (Some(renames), _, _) => {
            let parent = expand_tilde(&required(group.parent, "parent")?);
            let mut renames = plan_renames(renames)?;
            let naming = explicit_naming(group.source_volume_id, group.display_name)?;
            let parent = placer
                .place(
                    PlaceField::Parent,
                    LocationInput {
                        volume_id: Some(naming.source_volume_id.clone()),
                        path: parent,
                    },
                )?
                .path;
            let on = placer.sources(
                PlaceField::Renames,
                &naming.source_volume_id,
                renames.iter_mut().map(|rename| &mut rename.source_path),
            )?;
            PlannedOps::Rename {
                parent,
                renames,
                naming: naming.on(on),
            }
        }
        (None, Some(paths), _) => {
            let mut ops = plan_paths(paths)?;
            let naming = explicit_naming(group.source_volume_id, group.display_name)?;
            let shape = plan_shape(
                group.verb,
                group.destination,
                group.overwrites_existing,
                |destination| placer.place(PlaceField::Destination, destination),
            )?;
            let on = placer.sources(
                PlaceField::Paths,
                &naming.source_volume_id,
                ops.iter_mut().map(|op| &mut op.source_path),
            )?;
            PlannedOps::Sources {
                shape,
                sources: PlannedSources::Paths {
                    ops,
                    naming: naming.on(on),
                },
            }
        }
        (None, None, Some(selector)) => {
            let shape = plan_shape(
                group.verb,
                group.destination,
                group.overwrites_existing,
                |destination| placer.place(PlaceField::Destination, destination),
            )?;
            let selector = plan_selector(selector, now, |root| placer.place(PlaceField::SelectorRoot, root))?;
            PlannedOps::Sources {
                shape,
                sources: PlannedSources::Selector(selector),
            }
        }
        // The source count above admits exactly one, so this arm is unreachable; it answers
        // with the same typed refusal rather than a panic.
        (None, None, None) => return Err(GroupProblem::NoSources),
    };

    Ok(PlannedGroup {
        group_id: group.group_id,
        rationale: group.rationale,
        ops,
        via: placer.finish()?,
    })
}

/// The volume and title a hand-listed group must carry, or the typed refusal naming the one
/// that's missing.
fn explicit_naming(
    source_volume_id: Option<String>,
    display_name: Option<String>,
) -> Result<ExplicitNaming, GroupProblem> {
    Ok(ExplicitNaming {
        source_volume_id: required(source_volume_id, "sourceVolumeId")?,
        display_name: required(display_name, "displayName")?,
    })
}

/// A destination a group may be built with, or the refusal that says why not. Asked of the
/// PLACED path, so a re-addressed destination is checked where it will really write.
fn writable(location: Location) -> Result<WritableDestination, GroupProblem> {
    WritableDestination::new(location).ok_or(GroupProblem::DestinationTakesNoWrites)
}

/// Pair a verb with the target its executor binds: the per-verb executor table
/// (`../../store/proposals/DETAILS.md`), as code. `place` resolves the destination, and runs
/// only for a verb that binds one.
fn plan_shape(
    verb: ProposalVerb,
    destination: Option<LocationInput>,
    overwrites_existing: Option<bool>,
    place: impl FnOnce(LocationInput) -> Result<Location, GroupProblem>,
) -> Result<SourceShape, GroupProblem> {
    match verb {
        ProposalVerb::Move | ProposalVerb::Copy | ProposalVerb::Extract => {
            if overwrites_existing.is_some() {
                return Err(GroupProblem::UnboundField {
                    field: "overwritesExisting",
                });
            }
            let destination = writable(place(required(destination, "destination")?)?)?;
            Ok(match verb {
                ProposalVerb::Move => SourceShape::Move { destination },
                ProposalVerb::Copy => SourceShape::Copy { destination },
                _ => SourceShape::Extract { destination },
            })
        }
        ProposalVerb::Compress => Ok(SourceShape::Compress {
            // The archive being created IS the target and is perfectly legal; only a path
            // continuing inside one is refused.
            archive: writable(place(required(destination, "destination")?)?)?,
            overwrites_existing: overwrites_existing.unwrap_or(false),
        }),
        ProposalVerb::Trash | ProposalVerb::Delete => {
            if destination.is_some() {
                return Err(GroupProblem::UnboundField { field: "destination" });
            }
            if overwrites_existing.is_some() {
                return Err(GroupProblem::UnboundField {
                    field: "overwritesExisting",
                });
            }
            Ok(if verb == ProposalVerb::Trash {
                SourceShape::Trash
            } else {
                SourceShape::Delete
            })
        }
        // The caller routes a rename to its own op shape before reaching here, so this arm
        // exists only to keep the match total.
        ProposalVerb::Rename => Err(GroupProblem::RenamesVerbMismatch),
    }
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T, GroupProblem> {
    value.ok_or(GroupProblem::MissingField { field })
}

fn plan_paths(paths: Vec<String>) -> Result<Vec<NewOp>, GroupProblem> {
    if paths.is_empty() {
        return Err(GroupProblem::EmptySources);
    }
    if paths.len() > MAX_PATHS {
        return Err(GroupProblem::TooManyPaths { sent: paths.len() });
    }
    paths
        .into_iter()
        .map(|path| {
            Ok(NewOp {
                source_path: absolute_path(path)?,
                snapshot: None,
            })
        })
        .collect()
}

fn plan_renames(renames: Vec<RenameInput>) -> Result<Vec<NewRename>, GroupProblem> {
    if renames.is_empty() {
        return Err(GroupProblem::EmptySources);
    }
    if renames.len() > MAX_PATHS {
        return Err(GroupProblem::TooManyPaths { sent: renames.len() });
    }
    renames
        .into_iter()
        .map(|rename| {
            if rename.new_name.is_empty() || rename.new_name.contains('/') {
                return Err(GroupProblem::NotABareName { name: rename.new_name });
            }
            Ok(NewRename {
                source_path: absolute_path(rename.path)?,
                new_name: rename.new_name,
                snapshot: None,
            })
        })
        .collect()
}

/// A path an executor can take: absolute after tilde expansion, or a `scheme://` virtual
/// path (an archive's inside, an MTP device). ❌ A relative path is refused rather than
/// resolved — there is no working directory a proposal is relative to, and guessing one is
/// how a suggestion ends up naming a file the user never saw.
fn absolute_path(path: String) -> Result<String, GroupProblem> {
    let expanded = expand_tilde(&path);
    let is_virtual = expanded
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric()));
    if expanded.starts_with('/') || is_virtual {
        Ok(expanded)
    } else {
        Err(GroupProblem::RelativePath { path })
    }
}

/// Turn the model's selector into the store's, converting whole days ago into the unix
/// seconds the index compares against.
///
/// ❌ There is deliberately no "last opened" predicate: the drive index carries size,
/// modification time, and inode but no access time, and `importance.db`'s visit counts are
/// per-FOLDER. A predicate that silently matched nothing would be worse than its absence,
/// because the agent would propose over it and the user would review an empty group.
fn plan_selector(
    input: SelectorInput,
    now: i64,
    place: impl FnOnce(LocationInput) -> Result<Location, GroupProblem>,
) -> Result<OpSelector, GroupProblem> {
    if let (Some(min), Some(max)) = (input.min_size_bytes, input.max_size_bytes)
        && min > max
    {
        return Err(GroupProblem::ImpossibleWindow);
    }
    let modified_before = input.older_than_days.map(|days| days_ago(now, days));
    let modified_after = input.newer_than_days.map(|days| days_ago(now, days));
    // Two ages BAND a range: "older than 30 and newer than 90" means 30 to 90 days old, and
    // is legitimate. The reverse ("older than 90 and newer than 30") is empty, and proposing
    // over an empty window costs the user a review that can't contain anything.
    if let (Some(before), Some(after)) = (modified_before, modified_after)
        && after >= before
    {
        return Err(GroupProblem::ImpossibleWindow);
    }
    Ok(OpSelector {
        // Placed last, once the window is known to be satisfiable.
        root: place(input.root)?,
        name_glob: input.name_glob,
        min_size: input.min_size_bytes,
        max_size: input.max_size_bytes,
        modified_before,
        modified_after,
    })
}

/// The unix second `days` whole days before `now`, floored at the epoch so an absurd day
/// count can't wrap into the future.
fn days_ago(now: i64, days: u32) -> i64 {
    now.saturating_sub(i64::from(days).saturating_mul(SECONDS_PER_DAY))
        .max(0)
}
