//! Start voices for the zones selected by a note.
use kontra_behaviour::Step;

use crate::*;

impl Engine {
    /// The first condition that stops `zone` from starting for this note, in
    /// the order [`Engine::start_zones`] tests them; `None` when all hold.
    ///
    /// `accepted` is the caller's verdict on the zone's trigger, so the
    /// trigger test stays where the release/legato decision is made.
    fn refusal(&self, slot: usize, n: &Note, zone: &Zone, accepted: bool) -> Option<Reject> {
        if !accepted {
            return Some(Reject::Trigger);
        }
        if !self.masks.allows(slot, zone.group) {
            return Some(Reject::GroupMask);
        }
        if self
            .groups
            .get(zone.group as usize)
            .is_some_and(|g| g.purged)
        {
            return Some(Reject::Purged);
        }
        if !zone.keys.contains(&n.key) {
            return Some(Reject::KeyRange);
        }
        if !zone.velocities.contains(&n.velocity_byte) {
            return Some(Reject::Velocity);
        }
        if n.zone_slot >= 0 && n.zone_slot != i32::from(zone.slot) {
            return Some(Reject::ZoneSlot);
        }
        if !self
            .buses
            .get(plan::bus(&self.groups, zone))
            .is_some_and(|b| b.enabled)
        {
            return Some(Reject::BusDisabled);
        }
        None
    }

    pub(crate) fn start_zones(
        &mut self,
        note: NoteRef,
        attack: bool,
        accept: impl Fn(Trigger) -> bool,
    ) {
        let Some(n) = self.notes.get(note).copied() else {
            return;
        };
        let plan = self.plan.clone();
        let key = usize::from(n.key);
        if attack && let Some(r) = self.behaviour.rls_reset.get_mut(key) {
            *r = self.now.0;
        }
        let step = if attack { Step::Advance } else { Step::Reuse };
        let since = self.behaviour.rls_reset.get(key).map_or(n.at.0, |&r| r);
        let held_seconds = self.now.0.saturating_sub(since) as f32 / self.rate as f32;
        for (index, zone) in plan.zones.iter().enumerate() {
            let refusal = self.refusal(note.slot(), &n, zone, accept(zone.trigger));
            #[cfg(feature = "trace")]
            if self.collecting_rejects
                && let Some(reason) = refusal
            {
                self.rejects.push(crate::reject::Record {
                    zone: Some(index),
                    key: n.key,
                    velocity: n.velocity_byte,
                    event: note.event_id(),
                    attack,
                    reason,
                });
            }
            if refusal.is_some() {
                continue;
            }
            if !self
                .behaviour
                .starts(zone, n.host.channel, n.key, n.draw, step)
            {
                #[cfg(feature = "trace")]
                if self.collecting_rejects {
                    self.rejects.push(crate::reject::Record {
                        zone: Some(index),
                        key: n.key,
                        velocity: n.velocity_byte,
                        event: note.event_id(),
                        attack,
                        reason: crate::reject::Reject::StartRule,
                    });
                }
                continue;
            }
            self.choke_by(zone.group, note);
            self.limit_group(zone.group);
            if let Some(vg) = zone.voice_group {
                self.limit_voice_group(vg);
            }
            let factor = if zone.trigger == Trigger::Release {
                crate::ir::common::DB_GAIN.eval_unclamped(-zone.release_decay_db * held_seconds)
                    * self.release_counter(zone, held_seconds)
            } else {
                1.0
            };
            #[cfg(feature = "trace")]
            if let Some(trace) = &self.trace {
                trace.routes.begin_onset(index);
            }
            let mut region = zone.region.clone();
            if !region.reverse {
                let script = u64::from(n.offset) * u64::from(zone.sample_rate) / 1_000_000;
                let frames = self.start_offset(zone, &n, held_seconds);
                region.start = region
                    .start
                    .saturating_add(script)
                    .saturating_add_signed(frames)
                    .min(region.end);
            }
            if let [start, length] = self.loop_shift(zone, &n)
                && (start != 0.0 || length != 0.0)
                && !region.reverse
            {
                region.shift_loops(start, length);
            }
            if let Some(mix) = self.voice_mix(zone, &n, self.age(self.now, None)) {
                region.mix = Some(mix);
            }
            let pan = self.group_pan(zone);
            let (gain, lag) = self.voice_gain(zone, &n, factor, self.age(self.now, None), pan);
            let (step, bend) = self.step(zone, &n, self.age(self.now, None));
            let start = VoiceStart {
                region,
                step: voice_gain::bent(self.plan.route_pitch, step, bend.instant + bend.semitones),
                gain,
                lag,
                envelope: self.envelope(zone, &n, held_seconds),
                fade: n.fade.filter(|_| attack),
            };
            let now = self.now;
            let Some(i) = self.free_slot() else {
                #[cfg(feature = "trace")]
                if let Some(trace) = &self.trace {
                    trace.routes.finish_onset(None);
                }
                continue;
            };
            #[cfg(feature = "trace")]
            if let Some(trace) = &self.trace {
                trace.routes.finish_onset(Some(i));
            }
            #[cfg(feature = "trace")]
            if self.trace.is_some()
                && let Some(group) = self.groups.get(zone.group as usize)
            {
                for m in group.mods.iter().filter(|m| {
                    !m.bypass
                        && matches!(
                            m.kind,
                            ir::ModKind::SampleStart { .. }
                                | ir::ModKind::Attack { .. }
                                | ir::ModKind::Release { .. }
                                | ir::ModKind::LoopShift { .. }
                                | ir::ModKind::LfoPhase { .. }
                                | ir::ModKind::LfoRate { .. }
                        )
                }) {
                    let source = plan::route_source(m.kind).and_then(|s| match (m.kind, s) {
                        (
                            ir::ModKind::SampleStart { .. }
                            | ir::ModKind::Attack { .. }
                            | ir::ModKind::Release { .. },
                            ir::Source::HeldTime { span_ms },
                        ) => Some(self.inverted(m.invert, plan::held_time(span_ms, held_seconds))),
                        (
                            ir::ModKind::LfoPhase { .. } | ir::ModKind::LfoRate { .. },
                            ir::Source::Lfo(_),
                        ) => None,
                        _ => self
                            .source(zone, s, &n, modulators::Age::START)
                            .map(|x| self.inverted(m.invert, x)),
                    });
                    if let Some(trace) = &mut self.trace {
                        trace.push(TraceEvent::RouteOnset {
                            frame: now.0,
                            voice_slot: i,
                            zone: index,
                            mod_slot: m.slot,
                            target: m.target,
                            kind: m.kind,
                            source,
                        });
                    }
                }
            }
            let Some(slot) = self.voices.get_mut(i) else {
                continue;
            };
            #[cfg(feature = "trace")]
            if let Some(trace) = &mut self.trace {
                trace.push(TraceEvent::Voice {
                    frame: now.0,
                    slot: i,
                    zone: index,
                    event: VoiceEvent::Start,
                    position: start.region.start as f64,
                    step: start.step,
                    gain: start.gain.map(|row| row.map(|v| v * start.lag.gain)),
                });
            }
            slot.voice.start(start);
            slot.note = Some(note);
            slot.zone = index;
            slot.started = now;
            slot.factor = factor;
            slot.bend = [bend.semitones; 2];
            slot.pan = [pan; 2];
            slot.warp = [0.0; modulators::WARPS];
            slot.chaos = [modulators::CHAOS_START; modulators::WARPS];
            #[cfg(feature = "trace")]
            slot.pending_trace_cc.set(0);
            #[cfg(feature = "trace")]
            if let Some(trace) = &self.trace {
                trace.routes.begin_onset(index);
            }
            self.start_rack(i, zone, &n, self.age(now, None));
            #[cfg(feature = "trace")]
            if let Some(trace) = &self.trace {
                trace.routes.finish_onset(Some(i));
            }
        }
    }
}
