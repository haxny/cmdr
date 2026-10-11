//! Reordering the line: a waiting operation trades places with the nearest
//! waiting one, so the next lane to free admits the one the user put in front.
//! `../DETAILS.md` § "Reordering the line".

use crate::ignore_poison::IgnorePoison;

use super::super::types::LifecycleStatus;
use super::{OperationManager, QueueMoveDirection, QueueMoveOutcome};

impl OperationManager {
    /// Moves a waiting op one place earlier or later in line: it trades `order`
    /// slots with the nearest WAITING op on that side, so every running or
    /// paused op between them keeps its slot (their place in `order` decides
    /// nothing; they already hold their lanes).
    ///
    /// No admission pass. Admission takes any waiting op whose lanes are free,
    /// and the pass that ran when the line last changed left none, so a reorder
    /// can't make one admittable; it only decides who goes FIRST when a lane
    /// frees. Broadcasts only when something moved.
    pub(crate) fn move_queued(&self, operation_id: &str, direction: QueueMoveDirection) -> QueueMoveOutcome {
        let outcome = {
            let mut inner = self.inner.lock_ignore_poison();
            let waiting_slots: Vec<usize> = inner
                .order
                .iter()
                .enumerate()
                .filter(|(_, id)| {
                    inner
                        .records
                        .get(*id)
                        .is_some_and(|rec| rec.status == LifecycleStatus::Queued)
                })
                .map(|(slot, _)| slot)
                .collect();
            let Some(index) = waiting_slots.iter().position(|&slot| inner.order[slot] == operation_id) else {
                return QueueMoveOutcome::NotQueued;
            };
            // The line is the waiting ops, and it's never anywhere near u32::MAX long.
            let waiting = u32::try_from(waiting_slots.len()).unwrap_or(u32::MAX);
            let place = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
            let target = match direction {
                QueueMoveDirection::Earlier => index.checked_sub(1),
                QueueMoveDirection::Later => Some(index + 1).filter(|&next| next < waiting_slots.len()),
            };
            match target {
                Some(target) => {
                    inner.order.swap(waiting_slots[index], waiting_slots[target]);
                    QueueMoveOutcome::Moved {
                        position: place(target),
                        waiting,
                    }
                }
                None => QueueMoveOutcome::AtEdge {
                    position: place(index),
                    waiting,
                },
            }
        };
        if let QueueMoveOutcome::Moved { position, waiting } = outcome {
            log::info!(target: "op_manager", "move queued op={operation_id} to {position} of {waiting}");
            self.emit_changed();
        }
        outcome
    }
}
