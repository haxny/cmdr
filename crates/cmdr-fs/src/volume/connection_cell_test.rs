use super::connection_cell::{CellState, ConnectionCell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Up,
    Down,
    NeedsUser,
}

impl CellState for State {
    const LIVE: Self = Self::Up;
    const LOST: Self = Self::Down;

    fn to_byte(self) -> u8 {
        self as u8
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Up,
            2 => Self::NeedsUser,
            _ => Self::Down,
        }
    }
}

#[test]
fn a_new_cell_is_on_the_first_generation() {
    let cell = ConnectionCell::new(State::Up);
    assert_eq!(cell.state(), State::Up);
    assert_eq!(cell.generation(), 0);
}

#[test]
fn a_loss_on_the_installed_generation_flips_once() {
    let cell = ConnectionCell::new(State::Up);
    assert!(cell.lose(0));
    assert_eq!(cell.state(), State::Down);
    assert!(
        !cell.lose(0),
        "the second caller meeting the same dead session moves nothing"
    );
    assert_eq!(cell.generation(), 0, "a loss is a state change, never a new generation");
}

#[test]
fn a_loss_from_a_replaced_generation_leaves_the_new_session_live() {
    let cell = ConnectionCell::new(State::Up);
    cell.lose(0);
    assert!(cell.install());
    assert_eq!(cell.generation(), 1);

    assert!(!cell.lose(0), "the old session's late error");
    assert_eq!(cell.state(), State::Up);
    assert!(cell.lose(1), "the new session's own loss still counts");
}

#[test]
fn a_loss_never_overrides_a_state_only_a_person_moves() {
    let cell = ConnectionCell::new(State::Up);
    cell.set(State::NeedsUser);
    assert!(!cell.lose(0));
    assert_eq!(cell.state(), State::NeedsUser);
}

#[test]
fn an_install_always_starts_a_new_generation() {
    let cell = ConnectionCell::new(State::Up);
    assert!(!cell.install(), "replacing a live session moves no state");
    assert_eq!(cell.generation(), 1, "but it is a new session");
}

#[test]
fn a_revive_moves_only_a_connection_that_was_down() {
    let cell = ConnectionCell::new(State::Up);
    assert!(!cell.revive());
    assert_eq!(cell.generation(), 0, "a probe that found it live starts nothing new");

    cell.set(State::Down);
    assert!(cell.revive());
    assert_eq!((cell.state(), cell.generation()), (State::Up, 1));
}

#[test]
fn a_plain_set_keeps_the_generation_and_reports_only_a_change() {
    let cell = ConnectionCell::new(State::Up);
    cell.install();
    assert!(cell.set(State::Down));
    assert!(!cell.set(State::Down));
    assert!(cell.set(State::Up));
    assert_eq!(cell.generation(), 1);
}
