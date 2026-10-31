//! Frame replay: a journal is a film. Folding the events stepwise yields
//! one `PresentationSnapshot` per event — the Film Room's timeline. The
//! frames are a pure function of the journal, so film review can never
//! disagree with what the authority actually did (I-17).

use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_state::{apply, ApplyMode, DriveState};
use serde::{Deserialize, Serialize};

use crate::snapshot::PresentationSnapshot;
use crate::threeform::{kind, present_event, ThreeForm};

/// One film frame: the event that fired, its three forms, and the full
/// state projection after the fold accepted it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    /// 1-based event position in the journal.
    pub index: usize,
    /// The event's vocabulary key.
    pub kind: &'static str,
    pub forms: ThreeForm,
    pub snapshot: PresentationSnapshot,
}

/// Project a journal into film frames, folding stepwise. Fails on the
/// first inadmissible event, exactly as the authority would.
pub fn frames(
    events: &[DriveEvent],
    policy: &Policy,
) -> Result<Vec<Frame>, playbook_core::PlaybookError> {
    let mut state: Option<DriveState> = None;
    let mut film = Vec::with_capacity(events.len());
    for (index, event) in events.iter().enumerate() {
        state = Some(apply(state.as_ref(), event, policy, ApplyMode::Audit)?);
        let snapshot = PresentationSnapshot::from_state(state.as_ref().expect("just applied"));
        film.push(Frame {
            index: index + 1,
            kind: kind(event),
            forms: present_event(event),
            snapshot,
        });
    }
    Ok(film)
}
