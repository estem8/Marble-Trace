use std::collections::{HashMap, HashSet};

use mt_model::sim::cars::CarIdxFrame;
use mt_model::sim::enums::{SessionState, TrackSurface};
use mt_model::sim::session::ResultPosition;

use super::{
    completed_laps_for_finish, departed_track, finish_sample, is_racing,
    retain_tow_finish_baselines, DriverEntry, FinishTiming, RaceTiming,
};

pub(super) struct FinishSession {
    pub(super) state: Option<SessionState>,
    pub(super) time: Option<f64>,
    pub(super) results_official: bool,
}

impl RaceTiming {
    pub(super) fn begin_session(&mut self, session_num: i32) {
        if self.timing_session_num != Some(session_num) {
            *self = Self {
                timing_session_num: Some(session_num),
                ..Self::default()
            };
        }
    }

    pub(super) fn observe_finish(
        &mut self,
        entries: &mut [DriverEntry],
        car_idx: &CarIdxFrame,
        results_positions_map: &HashMap<i32, &ResultPosition>,
        observed_wraps: &HashSet<i32>,
        evidenced_laps_by_car: HashMap<i32, i32>,
        session: Option<FinishSession>,
    ) {
        let finish_latch_was_armed = self.laps_at_checkered.is_some();
        let mut finish_flag_armed_this_tick = false;

        // Finish latch. A car has finished once it crosses the line after the flag came
        // out, which the lap counter records against a baseline snapshotted from the tick
        // before. Whatever is left over is classified when the race ends (`CoolDown`).
        //
        // What that baseline hangs on is the leader, not a flag bit. Neither bit marks the
        // moment the field starts finishing: `CarIdxSessionFlags` carries the flags shown
        // to a driver (black, blue, meatball) and never lights the checkered at all, and
        // the session-wide `SessionFlags` checkered lights when the sim shows the flag to
        // the *player's* car — in a fifteen-car race at the Red Bull Ring that was ten
        // seconds after eleven cars had already taken it, and all eleven went into the
        // baseline with their finishing lap already counted. A baseline they cannot exceed
        // is a badge that never appears, so the whole leading group stayed unmarked until
        // `CoolDown` classified it.
        //
        // The leader's own crossing is the flag. `SessionState::Checkered` is where the
        // watch starts, not where the latch arms: in a timed race the sim enters it when
        // the clock expires, a full lap before anyone finishes, so arming there latches
        // every car one crossing early. So the leader's lap count is recorded there and
        // the latch arms on the tick that count grows.
        if let Some(session) = session {
            let mut finish_samples = HashMap::with_capacity(entries.len());
            let eligible_car_indices: HashSet<i32> = entries
                .iter()
                .filter(|entry| {
                    !entry.is_retired
                        && (!entry.is_towed && is_racing(entry)
                            || departed_track(entry.car_idx, entry.track_surface, self))
                })
                .map(|entry| entry.car_idx)
                .collect();
            let eligible_to_cross =
                |entry: &DriverEntry| eligible_car_indices.contains(&entry.car_idx);
            // The leading lap still counts while its driver is on pit road.
            let can_set_leading_lap = |entry: &DriverEntry| {
                eligible_to_cross(entry)
                    || (!entry.is_retired
                        && !entry.is_towed
                        && entry.on_pit_road
                        && entry.track_surface != TrackSurface::NotInWorld)
            };
            let leader_laps = entries
                .iter()
                .filter(|entry| can_set_leading_lap(entry))
                .map(|entry| completed_laps_for_finish(entry, self))
                .max()
                .unwrap_or(0);
            let leader_before = entries
                .iter()
                .filter(|entry| can_set_leading_lap(entry))
                .filter(|entry| {
                    self.previous_location.get(&entry.car_idx).is_some_and(
                        |(surface, on_pit_road)| {
                            matches!(surface, TrackSurface::OnTrack | TrackSurface::OffTrack)
                                || (*on_pit_road && *surface != TrackSurface::NotInWorld)
                        },
                    )
                })
                .filter_map(|entry| self.previous_laps.get(&entry.car_idx).copied())
                .max()
                .unwrap_or(leader_laps);
            let live_advance_past_leader = |entry: &DriverEntry| {
                car_idx
                    .car_idx_laps_completed
                    .get(entry.car_idx as usize)
                    .is_some_and(|&laps| laps > leader_before)
            };
            let eligible_leader_crossing = |entry: &DriverEntry| {
                eligible_to_cross(entry)
                    || (can_set_leading_lap(entry)
                        && matches!(
                            entry.track_surface,
                            TrackSurface::OnTrack
                                | TrackSurface::OffTrack
                                | TrackSurface::AproachingPits
                        )
                        && live_advance_past_leader(entry))
            };
            let leader_crossing_laps: Vec<i32> = entries
                .iter()
                .filter(|entry| eligible_leader_crossing(entry))
                .filter(|entry| {
                    self.previous_laps
                        .get(&entry.car_idx)
                        .is_some_and(|&laps| laps == leader_before)
                })
                .filter_map(|entry| {
                    let completed = completed_laps_for_finish(entry, self);
                    let live_advance = live_advance_past_leader(entry);
                    let results_advance = matches!(
                        entry.track_surface,
                        TrackSurface::OnTrack | TrackSurface::OffTrack
                    ) && results_positions_map
                        .get(&entry.car_idx)
                        .and_then(|result| result.laps_complete)
                        .is_some_and(|laps| {
                            laps > self
                                .previous_result_laps
                                .get(&entry.car_idx)
                                .copied()
                                .unwrap_or(leader_before)
                        });

                    (completed > leader_before
                        && (live_advance
                            || results_advance
                            || observed_wraps.contains(&entry.car_idx)))
                    .then_some(completed)
                })
                .collect();
            let leader_crossing_observed = !leader_crossing_laps.is_empty();
            let flag_laps = leader_crossing_laps
                .iter()
                .copied()
                .max()
                .unwrap_or(leader_laps);

            // `CoolDown` is the backstop: a race can be ended from the black box or by the
            // host with nobody crossing the line at all.
            let checkered_is_out = matches!(session.state, Some(SessionState::CoolDown))
                || (matches!(session.state, Some(SessionState::Checkered))
                    && leader_crossing_observed);
            let race_has_ended = matches!(session.state, Some(SessionState::CoolDown));

            let flag_crossing =
                checkered_is_out && !race_has_ended && self.laps_at_checkered.is_none();
            finish_flag_armed_this_tick = flag_crossing;

            if checkered_is_out && self.laps_at_checkered.is_none() {
                // The leader needs no special case: on the tick it crosses, the previous
                // tick still holds the lap it was on, so the same comparison that latches
                // everyone behind latches the leader too.
                self.laps_at_checkered = Some(self.previous_laps.clone());
            }

            retain_tow_finish_baselines(self, car_idx);

            let mut crossing_times = HashMap::new();
            for entry in entries.iter() {
                let Some(sample) = finish_sample(entry, car_idx, session.time) else {
                    continue;
                };

                if let Some(crossing_time) = self
                    .previous_finish_samples
                    .get(&entry.car_idx)
                    .and_then(|previous| sample.crossing_since(previous))
                {
                    crossing_times.insert(entry.car_idx, crossing_time);
                }

                finish_samples.insert(entry.car_idx, sample);
            }

            let mut flag_time_upper_bound: Option<f64> = None;
            let mut all_flag_crossings_timed = true;
            if flag_crossing {
                let flag_crossers = entries
                    .iter()
                    .filter(|entry| eligible_leader_crossing(entry))
                    .filter(|entry| completed_laps_for_finish(entry, self) == flag_laps)
                    .filter(|entry| {
                        self.previous_laps
                            .get(&entry.car_idx)
                            .is_some_and(|&laps| laps < flag_laps)
                    });

                for entry in flag_crossers {
                    if let Some(&time) = crossing_times.get(&entry.car_idx) {
                        flag_time_upper_bound =
                            Some(flag_time_upper_bound.map_or(time, |bound| bound.min(time)));
                    } else {
                        all_flag_crossings_timed = false;
                    }
                }
            }

            let state = &mut *self;

            for entry in entries.iter_mut() {
                let completed = completed_laps_for_finish(entry, state);
                let baseline_before_crossing = state
                    .laps_at_checkered
                    .as_ref()
                    .and_then(|laps| laps.get(&entry.car_idx))
                    .copied();
                let live_completed = car_idx
                    .car_idx_laps_completed
                    .get(entry.car_idx as usize)
                    .copied()
                    .filter(|laps| *laps >= 0);
                let result_completed = results_positions_map
                    .get(&entry.car_idx)
                    .and_then(|result| result.laps_complete);
                let results_advanced = result_completed.is_some_and(|laps| {
                    laps > state
                        .previous_result_laps
                        .get(&entry.car_idx)
                        .copied()
                        .unwrap_or(baseline_before_crossing.unwrap_or(completed))
                });

                // Seed a late arrival from its first observed lap.
                let crossed_the_line = state.laps_at_checkered.as_mut().is_some_and(|laps| {
                    let baseline = laps.entry(entry.car_idx).or_insert(completed);

                    completed > *baseline
                });
                let crossed_on_live_telemetry = crossed_the_line
                    && baseline_before_crossing
                        .is_some_and(|baseline| live_completed.is_some_and(|laps| laps > baseline));
                let crossed_on_results_while_in_world = crossed_the_line
                    && !entry.on_pit_road
                    && !entry.is_towed
                    && matches!(
                        entry.track_surface,
                        TrackSurface::OnTrack | TrackSurface::OffTrack
                    )
                    && results_advanced
                    && baseline_before_crossing
                        .zip(result_completed)
                        .is_some_and(|(baseline, laps)| laps > baseline);
                let crossing_evidence = crossed_on_live_telemetry
                    || crossed_on_results_while_in_world
                    || (crossed_the_line && observed_wraps.contains(&entry.car_idx));

                let car = state.cars.entry(entry.car_idx).or_default();
                // A results-only zero completed laps maps to lap one, even for a car
                // that never started. Preserve the cooldown rule for a live lap one.
                let live_lap = car_idx.car_idx_lap.get(entry.car_idx as usize).copied();
                if is_racing(entry) && live_lap.is_some_and(|lap| lap >= 1) {
                    car.started = true;
                }

                let started = entry.lap > 1
                    || live_lap == Some(1)
                    || car.last_in_world_laps.is_some_and(|laps| laps > 0)
                    || car.started;
                let classified = race_has_ended && !entry.is_retired && started;

                let crossing_time = crossing_times.get(&entry.car_idx).copied();
                // With partial evidence, an untimed leader may have crossed earlier.
                let before_flag_bound = crossing_time
                    .zip(flag_time_upper_bound)
                    .map(|(time, bound)| time < bound);
                let crossed_before_flag = crossing_evidence
                    && all_flag_crossings_timed
                    && before_flag_bound == Some(true);
                let crossing_order_unknown = flag_crossing
                    && crossing_evidence
                    && completed < flag_laps
                    && (before_flag_bound.is_none()
                        || (!all_flag_crossings_timed && before_flag_bound == Some(true)));

                let finisher_crossed = crossing_evidence
                    && !entry.is_retired
                    && !crossed_before_flag
                    && !crossing_order_unknown;

                if crossing_evidence && !entry.is_retired && !car.finished {
                    if !finisher_crossed {
                        if let Some(laps) = state.laps_at_checkered.as_mut() {
                            laps.insert(entry.car_idx, completed);
                        }
                    } else {
                        car.finish_timing = Some(FinishTiming {
                            laps_completed: completed,
                            crossing_time,
                            official_gap: None,
                        });
                    }
                }

                if let Some((result, laps_completed)) = session
                    .results_official
                    .then(|| results_positions_map.get(&entry.car_idx))
                    .flatten()
                    .and_then(|result| {
                        Some((result, result.laps_complete.filter(|laps| *laps > 0)?))
                    })
                {
                    let crossing_time = car
                        .finish_timing
                        .filter(|timing| timing.laps_completed == laps_completed)
                        .and_then(|timing| timing.crossing_time);
                    car.finish_timing = Some(FinishTiming {
                        laps_completed,
                        crossing_time,
                        official_gap: result.time.filter(|time| time.is_finite() && *time >= 0.0),
                    });
                }

                entry.finish_timing = car.finish_timing;

                if finisher_crossed || classified {
                    if crossed_on_live_telemetry {
                        car.last_in_world_laps = Some(completed);
                    }

                    car.finished = true;
                    state.towed_cars.remove(&entry.car_idx);
                    state.tow_finish_baselines.remove(&entry.car_idx);
                    entry.is_towed = false;
                }

                entry.is_finished = car.finished;
            }

            state.previous_finish_samples = finish_samples;
        }

        for (&result_car_idx, result) in results_positions_map {
            if let Some(laps) = result.laps_complete.filter(|laps| *laps >= 0) {
                let result_is_ahead_of_live = car_idx
                    .car_idx_laps_completed
                    .get(result_car_idx as usize)
                    .is_some_and(|&live| live >= 0 && laps > live);
                let defer_until_resolved = (finish_latch_was_armed || finish_flag_armed_this_tick)
                    && result_is_ahead_of_live
                    && entries.iter().any(|entry| {
                        entry.car_idx == result_car_idx
                            && !entry.is_retired
                            && matches!(
                                entry.track_surface,
                                TrackSurface::OnTrack | TrackSurface::OffTrack
                            )
                    });

                if !defer_until_resolved {
                    self.previous_result_laps.insert(result_car_idx, laps);
                }
            }
        }

        self.previous_location = entries
            .iter()
            .map(|entry| (entry.car_idx, (entry.track_surface, entry.on_pit_road)))
            .collect();

        self.previous_laps = evidenced_laps_by_car;
    }
}
