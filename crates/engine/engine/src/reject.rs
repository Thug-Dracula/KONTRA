//! Why a note started no voice in a zone: recorded for offline diagnosis only.
//!
//! The recorder is compiled in only with the `trace` feature and does nothing
//! unless a caller turns it on with [`crate::Engine::collect_rejects`]. The
//! selection conditions themselves are the ones [`crate::Engine::start_zones`]
//! has always applied; this module only names them, so a report can say *which*
//! one stopped a zone instead of guessing from the plan.

/// The first condition a note's zone failed when it tried to start.
///
/// The order is the order `start_zones` tests them, so the first `Some` is the
/// condition that actually stopped the zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reject {
    /// The note's trigger is not one this start accepts (a release zone on a
    /// note-on, or a legato zone on the first held note).
    Trigger,
    /// A script's `allow_group`/`disallow_group` mask excludes the zone's group
    /// for this note.
    GroupMask,
    /// The group is purged, so it holds no samples.
    Purged,
    /// The zone's key range does not contain the note's key.
    KeyRange,
    /// The zone's velocity range does not contain the note's velocity.
    Velocity,
    /// The note names a zone slot (`$EVENT_PAR_ZONE_ID`) and this zone is not in it.
    ZoneSlot,
    /// The group's bus is disabled, so nothing routed to it sounds.
    BusDisabled,
    /// Every condition above held, but the zone's start rule (keyswitch,
    /// controller range, round robin, cycle) did not.
    StartRule,
    /// A script called `ignore_event` on the note, so no zone was tried at all.
    Ignored,
    /// The note was released before its callback returned, so no zone was tried.
    Released,
}

impl Reject {
    /// The name a report prints for this reason.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Trigger => "trigger",
            Self::GroupMask => "group_mask",
            Self::Purged => "purged",
            Self::KeyRange => "key_range",
            Self::Velocity => "velocity",
            Self::ZoneSlot => "zone_slot",
            Self::BusDisabled => "bus_disabled",
            Self::StartRule => "start_rule",
            Self::Ignored => "ignored",
            Self::Released => "released",
        }
    }
}

/// One refusal: a note that could not start a zone, and why.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    /// The plan zone index, or `None` when the note never reached selection
    /// ([`Reject::Ignored`], [`Reject::Released`]).
    pub zone: Option<usize>,
    /// The key the note asked to sound.
    pub key: u8,
    /// Its MIDI velocity.
    pub velocity: u8,
    /// The note's script-visible event id, to tie the record to a callback.
    pub event: i32,
    /// The start pass: `true` for a note-on (attack and legato zones), `false`
    /// for the release pass at key-up (release zones).
    pub attack: bool,
    pub reason: Reject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_has_a_distinct_name() {
        let all = [
            Reject::Trigger,
            Reject::GroupMask,
            Reject::Purged,
            Reject::KeyRange,
            Reject::Velocity,
            Reject::ZoneSlot,
            Reject::BusDisabled,
            Reject::StartRule,
            Reject::Ignored,
            Reject::Released,
        ];
        let mut names: Vec<_> = all.iter().map(|r| r.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len(), "names {names:?}");
    }
}
