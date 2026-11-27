use std::collections::{HashMap, HashSet};

use mt_model::sim::cars::CarIdxFrame;
use mt_model::sim::enums::{SessionState, TrackSurface};
use mt_model::sim::session::ResultPosition;

use super::{
    completed_laps_for_finish, departed_track, finish_sample, is_racing,
    retain_tow_finish_baselines, CarRaceState, DriverEntry, FinishTiming, FrozenFinish,
    RaceOutcome, RaceTiming, DISQUALIFY,
};

pub(super) struct FinishSession {
    pub(super) state: Option<SessionState>,
    pub(super) time: Option<f64>,
    pub(super) results_official: bool,
}

fn record_finish(car: &mut CarRaceState, entry: &DriverEntry, timing: FinishTiming, tick: u64) {
    let completed = timing.laps_completed;
    if car.lap_history.number > completed {
        car.lap_history = Default::default();
    }
    car.lap_history.observe(completed, None, false, false);
    car.finished = true;
    car.finish_timing = Some(timing);
    car.finish = Some(FrozenFinish {
        completed,
        best_lap_time: entry.best_lap_time,
        live_time_at_crossing: car.last_live_lap_time,
        tick,
    });
}

fn apply_official_outcome(
    car: &mut CarRaceState,
    entry: &mut DriverEntry,
    result: &ResultPosition,
    tick: u64,
) {
    if result.reason_out_id.is_some_and(|reason| reason != 0) {
        let disqualified = result.reason_out_str.as_deref().is_some_and(|reason| {
            let reason = reason.to_ascii_lowercase();
            reason.contains("disqualif") || reason == "dq" || reason.starts_with("dq ")
        }) || car.disqualified;
        entry.race_outcome = Some(if disqualified {
            RaceOutcome::Dq
        } else {
            RaceOutcome::Dnf
        });
        return;
    }

    let Some(laps_completed) = result
        .laps_complete
        .filter(|&laps| laps > 0 || (laps == 0 && car.finished))
    else {
        return;
    };
    let crossing_time = car
        .finish_timing
        .filter(|timing| timing.laps_completed == laps_completed)
        .and_then(|timing| timing.crossing_time);
    let timing = FinishTiming {
        laps_completed,
        crossing_time,
        official_gap: result.time.filter(|time| time.is_finite() && *time >= 0.0),
    };

    if car.finish.is_none() && result.reason_out_id == Some(0) && result.position > 0 {
        record_finish(car, entry, timing, tick);
    }

    if let Some(finish) = car.finish.as_mut() {
        if finish.completed != laps_completed {
            if laps_completed < finish.completed {
                if car.lap_history.time.is_none() || finish.best_lap_time == car.lap_history.time {
                    finish.best_lap_time = None;
                }
                car.lap_history = Default::default();
            }
            car.lap_history.observe(laps_completed, None, false, false);
            finish.completed = laps_completed;
        }
        if laps_completed > 0 {
            finish.best_lap_time = valid_lap_time(result.fastest_time).or(finish.best_lap_time);
            if let Some(time) = valid_lap_time(result.last_time) {
                car.lap_history
                    .observe(laps_completed, Some(time), true, false);
            }
        } else {
            finish.best_lap_time = None;
        }
        car.finish_timing = Some(timing);
    }
}

fn apply_finish_snapshot(
    car: &mut CarRaceState,
    entry: &mut DriverEntry,
    result: Option<&ResultPosition>,
    live: Option<&CarIdxFrame>,
) {
    entry.is_towed = false;
    let Some(finish) = car.finish.as_mut() else {
        return;
    };
    if finish.completed > 0 && car.lap_history.time.is_none() {
        let result_time = valid_lap_time(
            result
                .filter(|result| result.laps_complete == Some(finish.completed))
                .and_then(|result| result.last_time),
        );
        let late_live_time = live.and_then(|frame| {
            let index = entry.car_idx as usize;
            if entry.track_surface == TrackSurface::NotInWorld
                || frame.car_idx_laps_completed.get(index) != Some(&finish.completed)
                || frame
                    .car_idx_lap
                    .get(index)
                    .is_some_and(|&lap| lap > 0 && lap != finish.completed.saturating_add(1))
            {
                return None;
            }
            valid_lap_time(frame.car_idx_last_lap_time.get(index).copied()).filter(|&time| {
                Some(time) != finish.live_time_at_crossing
                    && Some(time) != car.lap_history.previous_time
            })
        });
        let time = result_time.or(late_live_time);
        car.lap_history
            .observe(finish.completed, time, result_time.is_some(), false);
        if let Some(time) = time {
            finish.best_lap_time = Some(finish.best_lap_time.map_or(time, |best| best.min(time)));
        }
    }

    entry.is_finished = entry.race_outcome.is_none();
    entry.finish_timing = car.finish_timing.filter(|_| entry.is_finished);
    entry.laps_completed = finish.completed;
    entry.lap = finish.completed.saturating_add(1);
    entry.last_lap_time = car.lap_history.time;
    entry.last_lap_delta = car.lap_history.delta();
    entry.best_lap_time = finish.best_lap_time;
}

fn valid_lap_time(time: Option<f32>) -> Option<f32> {
    time.filter(|time| time.is_finite() && *time > 0.0)
}

impl RaceTiming {
    pub(super) fn begin_session(
        &mut self,
        sub_session_id: Option<i32>,
        session_num: i32,
        time: Option<f64>,
    ) {
        let key = (sub_session_id, session_num);
        let restarted = self.resuming
            && self
                .last_session_time
                .zip(time)
                .is_some_and(|(previous, current)| current < previous);
        if self.session_key != Some(key) || restarted {
            *self = Self {
                session_key: Some(key),
                ..Self::default()
            };
        }
        self.resuming = self.resuming && time.is_none() && self.last_session_time.is_some();
        self.last_session_time = time.or(self.last_session_time);
        self.tick += 1;
    }

    pub(super) fn disconnect(&mut self) {
        if !self.session_key.is_some_and(|(event, _)| event.is_some()) {
            *self = Self::default();
            return;
        }

        self.cars.retain(|_, car| car.finished);
        *self = Self {
            session_key: self.session_key,
            last_session_time: self.last_session_time,
            resuming: true,
            tick: self.tick,
            cars: std::mem::take(&mut self.cars),
            laps_at_checkered: self.laps_at_checkered.as_ref().map(|_| HashMap::new()),
            official_grid: std::mem::take(&mut self.official_grid),
            ..Self::default()
        };
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
        // Finish latch. A car has finished once it crosses the line after the flag came
        // out, which the lap counter records against a baseline snapshotted from the tick
        // before. The official results can classify a car whose crossing was missed.
        //
        // What that baseline hangs on is the leader, not a flag bit. Neither bit marks the
        // moment the field starts finishing: `CarIdxSessionFlags` carries the flags shown
        // to a driver (black, blue, meatball) and never lights the checkered at all, and
        // the session-wide `SessionFlags` checkered lights when the sim shows the flag to
        // the *player's* car — in a fifteen-car race at the Red Bull Ring that was ten
        // seconds after eleven cars had already taken it, and all eleven went into the
        // baseline with their finishing lap already counted. A baseline they cannot exceed
        // is a badge that never appears, so the whole leading group stayed unmarked until
        // the results classified it.
        //
        // The leader's own crossing is the flag. `SessionState::Checkered` is where the
        // watch starts, not where the latch arms: in a timed race the sim enters it when
        // the clock expires, a full lap before anyone finishes, so arming there latches
        // every car one crossing early. So the leader's lap count is recorded there and
        // the latch arms on the tick that count grows.
        if let Some(session) = session {
            let previous_session_state =
                std::mem::replace(&mut self.previous_session_state, session.state);
            let under_checkered = session.state == Some(SessionState::Checkered);
            let entering_cooldown = previous_session_state == Some(SessionState::Checkered)
                && session.state == Some(SessionState::CoolDown);
            self.observed_before_checkered |= matches!(
                session.state,
                Some(
                    SessionState::GetInCar
                        | SessionState::Warmup
                        | SessionState::ParadeLaps
                        | SessionState::Racing
                )
            );
            let mut finish_samples = HashMap::with_capacity(entries.len());
            let eligible_car_indices: HashSet<i32> = entries
                .iter()
                .filter(|entry| {
                    !entry.is_retired
                        && !self
                            .cars
                            .get(&entry.car_idx)
                            .is_some_and(|car| car.finished)
                        && (!entry.is_towed && is_racing(entry)
                            || departed_track(entry.car_idx, entry.track_surface, self))
                })
                .map(|entry| entry.car_idx)
                .collect();
            let eligible_to_cross =
                |entry: &DriverEntry| eligible_car_indices.contains(&entry.car_idx);
            // The leading lap still counts while its driver is on pit road.
            let pit_car_indices: HashSet<i32> = entries
                .iter()
                .filter(|entry| {
                    !entry.is_retired
                        && !entry.is_towed
                        && entry.on_pit_road
                        && entry.track_surface != TrackSurface::NotInWorld
                        && !self
                            .cars
                            .get(&entry.car_idx)
                            .is_some_and(|car| car.finished)
                })
                .map(|entry| entry.car_idx)
                .collect();
            let can_set_leading_lap = |entry: &DriverEntry| {
                eligible_to_cross(entry) || pit_car_indices.contains(&entry.car_idx)
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
                    under_checkered
                        || (entering_cooldown
                            && finish_sample(
                                entry,
                                car_idx,
                                session.time,
                                observed_wraps.contains(&entry.car_idx),
                            )
                            .zip(self.previous_finish_samples.get(&entry.car_idx))
                            .is_some_and(|(sample, previous)| {
                                sample.crossing_since(previous).is_some()
                            }))
                })
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

            // Attaching under the flag cannot tell a racing lap from the winner's rollout.
            let flag_crossing = self.observed_before_checkered
                && leader_crossing_observed
                && self.laps_at_checkered.is_none();
            if flag_crossing {
                // The leader needs no special case: on the tick it crosses, the previous
                // tick still holds the lap it was on, so the same comparison that latches
                // everyone behind latches the leader too.
                self.laps_at_checkered = Some(self.previous_laps.clone());
            }

            retain_tow_finish_baselines(self, car_idx);

            let mut crossing_times = HashMap::new();
            for entry in entries.iter() {
                if self
                    .cars
                    .get(&entry.car_idx)
                    .is_some_and(|car| car.finished)
                {
                    continue;
                }
                let Some(sample) = finish_sample(
                    entry,
                    car_idx,
                    session.time,
                    observed_wraps.contains(&entry.car_idx),
                ) else {
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
                let result = results_positions_map.get(&entry.car_idx).copied();
                let car = state.cars.entry(entry.car_idx).or_default();
                car.disqualified |= entry.raw_flags & DISQUALIFY != 0;
                if session.results_official {
                    if let Some(result) = result {
                        apply_official_outcome(car, entry, result, state.tick);
                    }
                }
                if car.finished || entry.race_outcome.is_some() {
                    apply_finish_snapshot(
                        car,
                        entry,
                        result,
                        (!session.results_official).then_some(car_idx),
                    );
                    state.towed_cars.remove(&entry.car_idx);
                    state.tow_finish_baselines.remove(&entry.car_idx);
                    continue;
                }

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
                let result_completed = result.and_then(|result| result.laps_complete);
                let results_advanced = result_completed.is_some_and(|laps| {
                    laps > state
                        .previous_result_laps
                        .get(&entry.car_idx)
                        .copied()
                        .unwrap_or(baseline_before_crossing.unwrap_or(completed))
                });

                // Seed a late arrival from its first observed lap.
                // The last live crossing can share the sample that enters CoolDown.
                let crossing_window = under_checkered
                    || (entering_cooldown && crossing_times.contains_key(&entry.car_idx));
                let crossed_the_line = crossing_window
                    && state.laps_at_checkered.as_mut().is_some_and(|laps| {
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

                if crossing_evidence && !entry.is_retired {
                    if !finisher_crossed {
                        if let Some(laps) = state.laps_at_checkered.as_mut() {
                            laps.insert(entry.car_idx, completed);
                        }
                    } else {
                        let car = state.cars.entry(entry.car_idx).or_default();
                        record_finish(
                            car,
                            entry,
                            FinishTiming {
                                laps_completed: completed,
                                crossing_time,
                                official_gap: None,
                            },
                            state.tick,
                        );
                        apply_finish_snapshot(car, entry, result, None);
                        state.towed_cars.remove(&entry.car_idx);
                        state.tow_finish_baselines.remove(&entry.car_idx);
                    }
                }

                if flag_crossing
                    && !finisher_crossed
                    && !entry.is_retired
                    && !entry.is_towed
                    && is_racing(entry)
                {
                    // A result already visible at the flag can belong to a
                    // crossing before it. A later counter catch-up cannot date it.
                    if let Some((baselines, result_laps)) =
                        state.laps_at_checkered.as_mut().zip(result_completed)
                    {
                        let baseline = baselines.entry(entry.car_idx).or_insert(completed);
                        *baseline = (*baseline).max(result_laps);
                    }
                }
            }

            state.previous_finish_samples = finish_samples;
        }

        for (&result_car_idx, result) in results_positions_map {
            if let Some(laps) = result.laps_complete.filter(|laps| *laps >= 0) {
                self.previous_result_laps.insert(result_car_idx, laps);
            }
        }

        self.previous_location = entries
            .iter()
            .map(|entry| (entry.car_idx, (entry.track_surface, entry.on_pit_road)))
            .collect();

        self.previous_laps = evidenced_laps_by_car;
    }
}
