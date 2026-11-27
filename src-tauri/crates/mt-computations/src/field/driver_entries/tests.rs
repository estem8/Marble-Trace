use super::*;
use mt_model::sim::enums::{PitState, TrackSurface};
use mt_model::sim::flags::CHECKERED;
use mt_model::sim::session::{CarEntry, QualifyResultEntry, SessionEntry};

// Most finish cases start their fixture under checkered to exercise its crossing
// logic. The processor has already observed the race in those scenarios.
fn prime_running_race(state: &Mutex<DriverEntriesState>, session: &SessionSnapshot) {
    let mut state = state.lock().unwrap();
    if state.race_timing.session_key.is_none() {
        state
            .race_timing
            .begin_session(session.sub_session_id, session.current_session_num, None);
        state.race_timing.observed_before_checkered = true;
    }
}

fn compute(
    car_idx: &CarIdxFrame,
    session: &SessionSnapshot,
    start_positions: &HashMap<i32, (i32, i32)>,
    compute_ir_delta: bool,
    session_state: Option<SessionState>,
    state: &Mutex<DriverEntriesState>,
) -> DriverEntriesFrame {
    if session_state == Some(SessionState::Checkered) {
        prime_running_race(state, session);
    }
    super::compute(
        car_idx,
        session,
        start_positions,
        compute_ir_delta,
        session_state,
        state,
    )
}

fn compute_with_clock(
    car_idx: &CarIdxFrame,
    session: &SessionSnapshot,
    start_positions: &HashMap<i32, (i32, i32)>,
    compute_ir_delta: bool,
    session_state: Option<SessionState>,
    state: &Mutex<DriverEntriesState>,
    clock: Option<f64>,
) -> DriverEntriesFrame {
    if session_state == Some(SessionState::Checkered) {
        prime_running_race(state, session);
    }
    super::compute_with_clock(
        car_idx,
        session,
        start_positions,
        compute_ir_delta,
        session_state,
        state,
        clock,
    )
}

fn garaged_car_idx_frame() -> CarIdxFrame {
    // What the sim reports for a car that left the world: everything cleared.
    CarIdxFrame {
        car_idx_lap_dist_pct: vec![-1.0],
        car_idx_on_pit_road: vec![false],
        car_idx_position: vec![0],
        car_idx_class_position: vec![0],
        car_idx_lap: vec![0],
        car_idx_laps_completed: vec![0],
        car_idx_last_lap_time: vec![-1.0],
        car_idx_best_lap_time: vec![-1.0],
        car_idx_f2_time: vec![0.0],
        car_idx_est_time: vec![0.0],
        car_idx_track_surface: vec![TrackSurface::NotInWorld],
        car_idx_tire_compound: vec![-1],
        car_idx_session_flags: vec![0],
        spotter: None,
    }
}

fn session_with_result(result: ResultPosition) -> SessionSnapshot {
    SessionSnapshot {
        player_car_idx: 0,
        current_session_num: 0,
        cars: vec![CarEntry {
            car_idx: 0,
            ..Default::default()
        }],
        sessions: vec![SessionEntry {
            results_positions: vec![result],
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn race_session() -> SessionSnapshot {
    SessionSnapshot {
        player_car_idx: 0,
        current_session_num: 0,
        cars: vec![CarEntry {
            car_idx: 0,
            ..Default::default()
        }],
        sessions: vec![SessionEntry {
            session_type: SessionType::Race,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn two_car_race_session() -> SessionSnapshot {
    let mut session = race_session();

    session.cars.push(CarEntry {
        car_idx: 1,
        ..Default::default()
    });

    session
}

/// Car 0 is the player, car 1 the race leader.
fn two_car_frame(leader_surface: TrackSurface, leader_pct: f32, player_pct: f32) -> CarIdxFrame {
    CarIdxFrame {
        car_idx_lap_dist_pct: vec![player_pct, leader_pct],
        car_idx_on_pit_road: vec![false, false],
        car_idx_position: vec![2, 1],
        car_idx_class_position: vec![2, 1],
        car_idx_lap: vec![1, 1],
        car_idx_laps_completed: vec![1, 1],
        car_idx_last_lap_time: vec![-1.0, -1.0],
        car_idx_best_lap_time: vec![-1.0, -1.0],
        car_idx_f2_time: vec![0.0, 0.0],
        car_idx_est_time: vec![0.0, 0.0],
        car_idx_track_surface: vec![TrackSurface::OnTrack, leader_surface],
        car_idx_tire_compound: vec![-1, -1],
        car_idx_session_flags: vec![0, 0],
        spotter: None,
    }
}

#[test]
fn test_classified_opponent_remains_after_live_fields_clear() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 1,
        position: 2,
        class_position: Some(2),
        last_time: Some(92.5),
        ..Default::default()
    });
    let mut car_idx = two_car_frame(TrackSurface::NotInWorld, -1.0, 0.3);
    car_idx.car_idx_position[0] = 1;
    car_idx.car_idx_position[1] = 0;
    car_idx.car_idx_class_position[1] = 0;

    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &Mutex::new(DriverEntriesState::default()),
    );

    let opponent = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    assert_eq!(opponent.position, 2);
    assert_eq!(opponent.class_position, 2);
    assert_eq!(opponent.last_lap_time, Some(92.5));
    assert_eq!(opponent.lap_dist_pct, None);
    assert_eq!(opponent.relative_lap_dist, None);
}

#[test]
fn test_a_towed_leader_loses_the_live_lead() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &two_car_frame(TrackSurface::OnTrack, 0.5, 0.3),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    // The truck picks the leader up straight off the track: it leaves the world
    // without ever having been on pit road.
    let frame = compute(
        &two_car_frame(TrackSurface::NotInWorld, -1.0, 0.35),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let leader = frame.entries.iter().find(|e| e.car_idx == 1).unwrap();

    assert!(leader.is_towed);

    // It is still ahead on the road it covered, so the order has not changed yet.
    assert_eq!(leader.live_position, 1);

    // Once the player drives past the point the leader was picked up at, the
    // live order must hand him the lead — the official one still says otherwise.
    let frame = compute(
        &two_car_frame(TrackSurface::NotInWorld, -1.0, 0.6),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let player = frame.entries.iter().find(|e| e.car_idx == 0).unwrap();

    assert_eq!(player.position, 2);
    assert_eq!(player.live_position, 1);
}

#[test]
fn test_a_car_entering_the_pits_is_not_treated_as_towed() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    let mut driving_in = two_car_frame(TrackSurface::OnTrack, 0.95, 0.3);
    driving_in.car_idx_on_pit_road = vec![false, true];

    compute(
        &driving_in,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let mut in_the_box = two_car_frame(TrackSurface::InPitStall, 0.97, 0.35);
    in_the_box.car_idx_on_pit_road = vec![false, true];

    let frame = compute(
        &in_the_box,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let leader = frame.entries.iter().find(|e| e.car_idx == 1).unwrap();

    assert!(!leader.is_towed);
    assert_eq!(leader.live_position, 1);
}

#[test]
fn test_tow_flag_clears_once_the_car_is_dropped_in_its_box() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &two_car_frame(TrackSurface::OnTrack, 0.5, 0.3),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let frame = compute(
        &two_car_frame(TrackSurface::NotInWorld, -1.0, 0.35),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert!(
        frame
            .entries
            .iter()
            .find(|e| e.car_idx == 1)
            .unwrap()
            .is_towed
    );

    // The truck sets it down in its pit box. It is not racing yet — and may
    // never race again — but the tow is over.
    let mut dropped_off = two_car_frame(TrackSurface::InPitStall, 0.97, 0.4);
    dropped_off.car_idx_on_pit_road = vec![false, true];

    let frame = compute(
        &dropped_off,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert!(
        !frame
            .entries
            .iter()
            .find(|e| e.car_idx == 1)
            .unwrap()
            .is_towed
    );
}

fn racing_car_idx_frame(flags: u32) -> CarIdxFrame {
    racing_car_idx_frame_on_lap(flags, 12)
}

fn racing_car_idx_frame_on_lap(flags: u32, lap: i32) -> CarIdxFrame {
    let mut car_idx = garaged_car_idx_frame();

    car_idx.car_idx_lap_dist_pct = vec![0.3];
    car_idx.car_idx_position = vec![1];
    car_idx.car_idx_lap = vec![lap];
    car_idx.car_idx_laps_completed = vec![lap - 1];
    car_idx.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car_idx.car_idx_session_flags = vec![flags];

    car_idx
}

#[test]
fn test_car_finishes_when_it_crosses_the_line_under_the_checkered() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &racing_car_idx_frame_on_lap(0, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    // The flag comes out on the tick the leader's lap counter ticks over.
    let frame = compute(
        &racing_car_idx_frame_on_lap(CHECKERED, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);

    // The latch holds once the sim drops the bit again.
    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);
}

#[test]
fn finish_timing_survives_departure_and_accepts_only_official_results() {
    let mut session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut before = racing_car_idx_frame_on_lap(0, 12);
    before.car_idx_lap_dist_pct = vec![0.99];
    let mut crossing = racing_car_idx_frame_on_lap(CHECKERED, 13);
    crossing.car_idx_lap_dist_pct = vec![0.01];
    let standings = |car_idx: &CarIdxFrame, session: &SessionSnapshot, time, session_state| {
        compute_with_clock(
            car_idx,
            session,
            &HashMap::new(),
            false,
            Some(session_state),
            &state,
            Some(time),
        )
    };

    standings(&before, &session, 600.0, SessionState::Racing);
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(11),
        time: Some(0.0),
        ..Default::default()
    });
    let finished = standings(&crossing, &session, 600.2, SessionState::Checkered);
    let timing = finished.entries[0].finish_timing.unwrap();
    assert_eq!(timing.laps_completed, 12);
    assert_eq!(timing.official_gap, None);
    assert!((timing.crossing_time.unwrap() - 600.1).abs() < 0.00001);

    let garage = garaged_car_idx_frame();
    let retained = standings(&garage, &session, 609.0, SessionState::CoolDown);
    assert_eq!(retained.entries[0].finish_timing, Some(timing));

    session.sessions[0].results_official = Some(true);
    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let official = standings(&garage, &session, 610.0, SessionState::CoolDown);
    assert_eq!(
        official.entries[0].finish_timing,
        Some(FinishTiming {
            official_gap: Some(0.0),
            ..timing
        })
    );

    session.current_session_num = 1;
    session.sessions.push(SessionEntry {
        session_type: SessionType::Race,
        ..Default::default()
    });
    let next = standings(&before, &session, 6.0, SessionState::Racing);
    assert_eq!(next.entries[0].finish_timing, None);
}

#[test]
fn test_car_absent_at_the_flag_can_still_finish() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    // The flag is already out on the very first tick we see: there is no
    // previous lap counter to build the baseline from.
    let frame = compute(
        &racing_car_idx_frame_on_lap(CHECKERED, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries[0].is_finished);

    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);
}

#[test]
fn test_the_leaders_crossing_arms_the_latch_with_no_flag_bit_at_all() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    // The clock has expired, so the sim sits in `Checkered` while the leader
    // still runs the last lap. Nothing is out yet.
    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries[0].is_finished);

    // The leader takes the flag — which is this car, a one-car field. Neither
    // flag field carries a bit; the lap counter ticking over is the whole signal.
    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);
}

#[test]
fn test_a_leader_crossing_during_racing_does_not_arm_the_finish_latch() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &racing_car_idx_frame_on_lap(0, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert!(!frame.entries[0].is_finished);
}

#[test]
fn test_a_lapped_car_crossing_before_the_flag_in_the_same_tick_stays_unfinished() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut before = trailing_and_leading_frame(12, 13);
    before.car_idx_lap_dist_pct = vec![0.995, 0.99];
    compute_with_clock(
        &before,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        Some(10.0),
    );

    let mut at_flag = trailing_and_leading_frame(13, 14);
    at_flag.car_idx_lap_dist_pct = vec![0.01, 0.01];
    let frame = compute_with_clock(
        &at_flag,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        Some(10.1),
    );
    assert!(
        frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 1)
            .unwrap()
            .is_finished
    );
    assert!(
        !frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );

    let mut after_flag = trailing_and_leading_frame(14, 14);
    after_flag.car_idx_lap_dist_pct = vec![0.02, 0.15];
    let frame = compute_with_clock(
        &after_flag,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        Some(10.2),
    );
    assert!(
        frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );
}

#[test]
fn a_car_catching_the_leader_on_pit_road_does_not_arm_the_flag() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut live = trailing_and_leading_frame(20, 21);
    live.car_idx_on_pit_road[1] = true;
    let sample = |live: &CarIdxFrame| {
        compute(
            live,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        )
    };
    sample(&live);

    live.car_idx_lap[0] = 21;
    live.car_idx_laps_completed[0] = 20;
    assert!(!player_entry(&sample(&live)).is_finished);

    live.car_idx_lap[0] = 22;
    live.car_idx_laps_completed[0] = 21;
    assert!(player_entry(&sample(&live)).is_finished);
}

#[test]
fn a_live_pit_road_crossing_by_the_leader_arms_the_flag() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |live: &CarIdxFrame, time| {
        compute_with_clock(
            live,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
            Some(time),
        )
    };
    let mut live = trailing_and_leading_frame(20, 21);
    live.car_idx_on_pit_road[1] = true;
    live.car_idx_track_surface[1] = TrackSurface::AproachingPits;
    live.car_idx_lap_dist_pct = vec![0.995, 0.99];
    sample(&live, 10.0);

    live.car_idx_lap = vec![21, 22];
    live.car_idx_laps_completed = vec![20, 21];
    live.car_idx_lap_dist_pct = vec![0.01, 0.01];
    let frame = sample(&live, 10.1);
    assert!(!player_entry(&frame).is_finished);
    assert!(frame
        .entries
        .iter()
        .any(|entry| entry.car_idx == 1 && entry.is_finished));

    live.car_idx_lap[0] = 22;
    live.car_idx_laps_completed[0] = 21;
    assert!(player_entry(&sample(&live, 10.2)).is_finished);
}

#[test]
fn a_timed_leader_crossing_bounds_the_flag_when_another_sample_is_missing() {
    let mut session = two_car_race_session();
    session.cars.push(CarEntry {
        car_idx: 2,
        ..Default::default()
    });

    for (trailing_progress, finished) in [(0.98, true), (0.995, false)] {
        let state = Mutex::new(DriverEntriesState::default());
        let mut live = trailing_and_leading_frame(12, 13);
        live.car_idx_position.push(3);
        live.car_idx_track_surface.push(TrackSurface::OnTrack);
        live.car_idx_lap.push(13);
        live.car_idx_laps_completed.push(12);
        live.car_idx_lap_dist_pct = vec![trailing_progress, 0.99, -1.0];
        let sample = |live: &CarIdxFrame, time| {
            compute_with_clock(
                live,
                &session,
                &HashMap::new(),
                false,
                Some(SessionState::Checkered),
                &state,
                Some(time),
            )
        };
        sample(&live, 10.0);

        live.car_idx_lap = vec![13, 14, 14];
        live.car_idx_laps_completed = vec![12, 13, 13];
        live.car_idx_lap_dist_pct = vec![0.01, 0.01, 0.01];
        let frame = sample(&live, 10.1);
        let trailing = player_entry(&frame);
        assert_eq!(trailing.is_finished, finished);
        assert_eq!(trailing.finish_timing.is_some(), finished);
    }
}

#[test]
fn test_unknown_same_tick_crossing_order_waits_for_later_evidence() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    compute_with_clock(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        None,
    );

    let crossing = compute_with_clock(
        &trailing_and_leading_frame(13, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        None,
    );
    assert!(
        crossing
            .entries
            .iter()
            .find(|entry| entry.car_idx == 1)
            .unwrap()
            .is_finished
    );
    assert!(
        !crossing
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );

    let unchanged = compute_with_clock(
        &trailing_and_leading_frame(13, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        Some(10.2),
    );
    assert!(
        !unchanged
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );

    let later = compute_with_clock(
        &trailing_and_leading_frame(14, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
        Some(10.3),
    );
    assert!(
        later
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );
}

/// Car 0 runs its own race, car 1 leads it by a lap. The finish latch hangs on
/// the leader's crossing, so the two have to move independently.
fn trailing_and_leading_frame(lap: i32, leader_lap: i32) -> CarIdxFrame {
    CarIdxFrame {
        car_idx_lap_dist_pct: vec![0.3, 0.6],
        car_idx_on_pit_road: vec![false, false],
        car_idx_position: vec![2, 1],
        car_idx_class_position: vec![2, 1],
        car_idx_lap: vec![lap, leader_lap],
        car_idx_laps_completed: vec![lap - 1, leader_lap - 1],
        car_idx_last_lap_time: vec![-1.0, -1.0],
        car_idx_best_lap_time: vec![-1.0, -1.0],
        car_idx_f2_time: vec![0.0, 0.0],
        car_idx_est_time: vec![0.0, 0.0],
        car_idx_track_surface: vec![TrackSurface::OnTrack, TrackSurface::OnTrack],
        car_idx_tire_compound: vec![-1, -1],
        car_idx_session_flags: vec![0, 0],
        spotter: None,
    }
}

#[test]
fn test_a_car_crossing_before_the_leader_does_not_finish() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    // A timed race whose clock has expired: the sim is already in `Checkered`,
    // but the leader is still on the last lap. Crossing the line here starts a
    // lap, it does not end the race.
    let frame = compute(
        &trailing_and_leading_frame(13, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries.iter().any(|entry| entry.is_finished));

    // The leader takes the flag and finishes on that very crossing.
    let frame = compute(
        &trailing_and_leading_frame(13, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    let trailing = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();

    assert!(leader.is_finished);
    assert!(!trailing.is_finished);

    // And the car behind finishes on its next crossing.
    let frame = compute(
        &trailing_and_leading_frame(14, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries.iter().all(|entry| entry.is_finished));
}

#[test]
fn test_a_car_that_crosses_before_the_flag_bit_lights_still_finishes() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(13, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    // Both cross within the same tick, the leader first. `SessionFlags` lights
    // the checkered bit for nobody — the sim shows it to the player's car, which
    // in a real race trailed this pair by ten seconds. Both have finished.
    let frame = compute(
        &trailing_and_leading_frame(14, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries.iter().all(|entry| entry.is_finished));
}

#[test]
fn test_a_finished_car_that_leaves_the_session_is_not_towed() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &racing_car_idx_frame_on_lap(0, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);

    // He quits straight from the racing surface, which is the shape of a tow.
    let mut car_idx = racing_car_idx_frame_on_lap(0, 13);
    car_idx.car_idx_track_surface = vec![TrackSurface::NotInWorld];

    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries[0].is_towed);
    assert!(frame.entries[0].is_finished);
}

#[test]
fn test_the_official_results_stand_in_for_a_zeroed_lap_counter() {
    let mut session = race_session();

    session.sessions[0].results_positions = vec![ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(12),
        ..Default::default()
    }];

    let state = Mutex::new(DriverEntriesState::default());

    let frame = compute(
        &racing_car_idx_frame_on_lap(0, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert_eq!(frame.entries[0].lap, 13);
    assert_eq!(frame.entries[0].laps_completed, 12);

    // The live counter goes to -1 and `ResultsPositions` takes over. Both count
    // completed laps, so the baseline of 12 is beaten by the 13 the results
    // report and the crossing is caught even though it was never seen live. The
    // car is left on track on purpose: this pins the fallback alone, with the
    // rule about leaving the world under the flag kept out of it.
    session.sessions[0].results_positions[0].laps_complete = Some(13);
    let mut car_idx = racing_car_idx_frame_on_lap(0, 13);
    car_idx.car_idx_laps_completed = vec![-1];

    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame.entries[0].is_finished);
    assert_eq!(frame.entries[0].laps_completed, 13);
    assert_eq!(frame.entries[0].lap, 14);
}

#[test]
fn test_post_flag_result_waits_for_live_count_to_clear_before_finishing() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let flag = compute(
        &trailing_and_leading_frame(12, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(
        !flag
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );

    let result_ahead = compute(
        &trailing_and_leading_frame(12, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(
        !result_ahead
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_finished
    );

    let mut live_cleared = trailing_and_leading_frame(12, 14);
    live_cleared.car_idx_laps_completed[0] = -1;
    let resolved = compute(
        &live_cleared,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let trailing_car = resolved
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert!(trailing_car.is_finished);
    assert!(trailing_car.finish_timing.is_some());
}

#[test]
fn test_out_of_world_car_uses_recorded_completed_laps() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(13),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());

    let frame = compute(
        &garaged_car_idx_frame(),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(frame.entries[0].track_surface, TrackSurface::NotInWorld);
    assert_eq!(frame.entries[0].laps_completed, 13);
    assert_eq!(frame.entries[0].lap, 14);

    session.sessions[0].results_positions[0].laps_complete = Some(0);
    let frame = compute(
        &garaged_car_idx_frame(),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(frame.entries[0].laps_completed, 0);
    assert_eq!(frame.entries[0].lap, 1);
}

#[test]
fn test_result_update_on_tow_departure_does_not_arm_the_flag() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(10),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 11),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let mut towed = trailing_and_leading_frame(12, 12);
    towed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    towed.car_idx_lap_dist_pct[0] = -1.0;
    towed.car_idx_lap[0] = 0;
    towed.car_idx_laps_completed[0] = 0;

    let frame = compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries.iter().any(|entry| entry.is_finished));
    assert!(frame.entries.iter().any(|entry| entry.is_towed));
    assert!(state
        .lock()
        .unwrap()
        .race_timing
        .laps_at_checkered
        .is_none());

    towed.car_idx_lap[1] = 13;
    towed.car_idx_laps_completed[1] = 12;
    let frame = compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(frame
        .entries
        .iter()
        .any(|entry| entry.car_idx == 1 && entry.is_finished));
    assert!(frame
        .entries
        .iter()
        .any(|entry| entry.car_idx == 0 && entry.is_towed && !entry.is_finished));
}

#[test]
fn test_results_only_pit_reentry_before_the_flag_does_not_arm_it() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(12, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let mut towed = trailing_and_leading_frame(12, 12);
    towed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    towed.car_idx_lap_dist_pct[0] = -1.0;
    towed.car_idx_lap[0] = 0;
    towed.car_idx_laps_completed[0] = 0;
    let frame = compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert!(player_entry(&frame).is_towed);

    towed.car_idx_track_surface[0] = TrackSurface::InPitStall;
    towed.car_idx_laps_completed[0] = -1;
    let returned = compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&returned).is_towed);
    assert!(!player_entry(&returned).is_finished);
    assert!(state
        .lock()
        .unwrap()
        .race_timing
        .laps_at_checkered
        .is_none());
    assert_eq!(
        state.lock().unwrap().race_timing.previous_laps.get(&1),
        Some(&11)
    );

    let leader_crosses = compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let active_leader = leader_crosses
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    assert_eq!(active_leader.laps_completed, 12);
    assert!(!active_leader.is_towed);
    assert!(!active_leader.is_retired);
    assert!(state
        .lock()
        .unwrap()
        .race_timing
        .laps_at_checkered
        .is_some());
    assert!(
        leader_crosses
            .entries
            .iter()
            .find(|entry| entry.car_idx == 1)
            .unwrap()
            .is_finished
    );
    assert!(!player_entry(&leader_crosses).is_finished);
}

#[test]
fn test_towed_car_result_update_does_not_arm_the_flag() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(10),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 11),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let mut towed = trailing_and_leading_frame(12, 11);
    towed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    towed.car_idx_lap_dist_pct[0] = -1.0;
    towed.car_idx_lap[0] = 0;
    towed.car_idx_laps_completed[0] = 0;
    compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    for completed in [10, 11, 12] {
        towed.car_idx_lap[1] = completed + 1;
        towed.car_idx_laps_completed[1] = completed;
        let frame = compute(
            &towed,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        );

        let towed_entry = frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap();
        let racing_entry = frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 1)
            .unwrap();
        assert!(towed_entry.is_towed);
        assert_eq!(towed_entry.laps_completed, 12);
        assert!(!towed_entry.is_finished);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .race_timing
                .laps_at_checkered
                .is_some(),
            completed >= 11
        );
        assert_eq!(racing_entry.is_finished, completed >= 11);
    }
}

#[test]
fn test_pre_flag_tow_without_results_stays_unconfirmed_at_cooldown() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let mut towed = trailing_and_leading_frame(12, 12);
    towed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    towed.car_idx_lap_dist_pct[0] = -1.0;
    towed.car_idx_lap[0] = 0;
    towed.car_idx_laps_completed[0] = 0;
    let frame = compute(
        &towed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert!(
        frame
            .entries
            .iter()
            .find(|entry| entry.car_idx == 0)
            .unwrap()
            .is_towed
    );

    let mut flag_out = trailing_and_leading_frame(12, 13);
    flag_out.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    flag_out.car_idx_lap_dist_pct[0] = -1.0;
    flag_out.car_idx_lap[0] = 0;
    flag_out.car_idx_laps_completed[0] = 0;
    let frame = compute(
        &flag_out,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let player = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert!(player.is_towed);
    assert!(!player.is_finished);

    let frame = compute(
        &flag_out,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &state,
    );
    let player = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert!(!player.is_finished);
    assert!(player.is_towed);

    let frame = compute(
        &flag_out,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &Mutex::new(DriverEntriesState::default()),
    );
    let player = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert!(!player.is_finished);
}

#[test]
fn test_leader_crossing_on_departure_tick_arms_finish_latch() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(11, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let mut departed = trailing_and_leading_frame(11, 13);
    departed.car_idx_track_surface[1] = TrackSurface::NotInWorld;
    departed.car_idx_lap_dist_pct[1] = -1.0;
    departed.car_idx_lap[1] = 0;
    let frame = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    assert!(leader.is_finished);
    assert!(!leader.is_towed);
}

#[test]
fn test_retired_car_does_not_latch_a_finish_from_a_result_update() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 2,
        laps_complete: Some(11),
        reason_out_id: Some(0),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    session.sessions[0].results_positions[0].reason_out_id = Some(2);
    let frame = compute(
        &trailing_and_leading_frame(13, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let retired = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert!(retired.is_retired);
    assert!(!retired.is_finished);
}

#[test]
fn test_retired_results_do_not_raise_the_lap_needed_to_arm_the_flag() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(13),
        reason_out_id: Some(1),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(13, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let crossing = compute(
        &trailing_and_leading_frame(14, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(
        crossing
            .entries
            .iter()
            .find(|entry| entry.car_idx == 1)
            .unwrap()
            .is_finished
    );
}

#[test]
fn test_post_flag_departure_with_live_crossing_finishes_retained_car() {
    assert_post_flag_departure(12, Some(11), true, 12);
}

#[test]
fn test_post_flag_departure_with_results_update_only_stays_towed() {
    assert_post_flag_departure(0, Some(12), false, 12);
}

#[test]
fn test_post_flag_departure_with_cleared_lap_count_keeps_last_live_count() {
    assert_post_flag_departure(0, None, false, 11);
    assert_post_flag_departure(0, Some(10), false, 11);
}

fn assert_post_flag_departure(
    live_completed_laps: i32,
    result_laps: Option<i32>,
    expected_finished: bool,
    expected_laps_completed: i32,
) {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 2,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let mut departed = trailing_and_leading_frame(12, 13);
    departed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    departed.car_idx_lap_dist_pct[0] = -1.0;
    departed.car_idx_lap[0] = 0;
    departed.car_idx_laps_completed[0] = live_completed_laps;
    session.sessions[0]
        .results_positions
        .last_mut()
        .unwrap()
        .laps_complete = result_laps;
    let frame = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let driver = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert_eq!(driver.is_finished, expected_finished);
    assert_eq!(driver.is_towed, !expected_finished);
    assert_eq!(driver.laps_completed, expected_laps_completed);
    assert_eq!(driver.lap, expected_laps_completed + 1);

    departed.car_idx_laps_completed[0] = 0;
    let next = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let driver = player_entry(&next);
    assert_eq!(driver.laps_completed, expected_laps_completed);
    assert_eq!(driver.lap, expected_laps_completed + 1);
    assert_eq!(driver.is_finished, expected_finished);

    session.sessions.push(session.sessions[0].clone());
    session.current_session_num = 1;
    let reset = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(
        player_entry(&reset).laps_completed,
        result_laps.unwrap_or(0)
    );
}

#[test]
fn test_unavailable_live_count_keeps_departure_history_but_zero_replaces_it() {
    for (next_count, expected_lap, expected_completed) in [(-1, 12, 11), (0, 1, 0)] {
        let session = race_session();
        let state = Mutex::new(DriverEntriesState::default());
        let standings = |frame: &CarIdxFrame| {
            compute(
                frame,
                &session,
                &HashMap::new(),
                false,
                Some(SessionState::Racing),
                &state,
            )
        };

        standings(&racing_car_idx_frame_on_lap(0, 12));
        let mut on_track = racing_car_idx_frame_on_lap(0, 12);
        on_track.car_idx_laps_completed[0] = next_count;
        standings(&on_track);

        let departed = standings(&garaged_car_idx_frame());
        assert_eq!(player_entry(&departed).laps_completed, expected_completed);
        assert_eq!(player_entry(&departed).lap, expected_lap);

        let without_history = compute(
            &garaged_car_idx_frame(),
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Racing),
            &Mutex::new(DriverEntriesState::default()),
        );
        assert_eq!(player_entry(&without_history).lap, 0);
    }
}

#[test]
fn test_missing_live_count_does_not_let_an_older_result_lower_history() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut live = garaged_car_idx_frame();
    live.car_idx_track_surface = vec![TrackSurface::OnTrack];
    live.car_idx_lap = vec![13];
    live.car_idx_laps_completed = vec![12];
    live.car_idx_lap_dist_pct = vec![0.4];

    compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    live.car_idx_laps_completed[0] = -1;
    session.sessions[0].results_positions[0].laps_complete = Some(11);
    let frame = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(player_entry(&frame).laps_completed, 12);
    assert_eq!(
        state.lock().unwrap().race_timing.cars[&0].last_in_world_laps,
        Some(12)
    );
}

#[test]
fn test_infers_the_current_lap_from_live_completed_laps() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let mut live = garaged_car_idx_frame();
    live.car_idx_track_surface = vec![TrackSurface::OnTrack];
    live.car_idx_lap = vec![0];
    live.car_idx_laps_completed = vec![12];
    live.car_idx_lap_dist_pct = vec![0.4];
    let frame = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &Mutex::new(DriverEntriesState::default()),
    );

    assert_eq!(player_entry(&frame).laps_completed, 12);
    assert_eq!(player_entry(&frame).lap, 13);
}

#[test]
fn test_infers_completed_laps_from_live_lap_when_the_counter_is_unavailable() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut live = garaged_car_idx_frame();
    live.car_idx_track_surface = vec![TrackSurface::OnTrack];
    live.car_idx_lap = vec![13];
    live.car_idx_laps_completed = vec![-1];
    live.car_idx_lap_dist_pct = vec![0.4];

    let frame = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(player_entry(&frame).laps_completed, 12);

    live.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    live.car_idx_lap[0] = 0;
    live.car_idx_lap_dist_pct[0] = -1.0;
    let departed = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(player_entry(&departed).laps_completed, 12);
}

#[test]
fn test_departure_after_pit_road_keeps_a_new_live_completed_lap() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut in_pit_road = trailing_and_leading_frame(12, 12);
    in_pit_road.car_idx_on_pit_road[0] = true;

    compute(
        &in_pit_road,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    let mut departed = in_pit_road;
    departed.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    departed.car_idx_lap_dist_pct[0] = -1.0;
    departed.car_idx_lap[0] = 0;
    departed.car_idx_laps_completed[0] = 12;
    departed.car_idx_last_lap_time[0] = 93.1;
    let departure = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(player_entry(&departure).laps_completed, 12);
    assert_eq!(player_entry(&departure).last_lap_time, Some(93.1));

    departed.car_idx_laps_completed[0] = 0;
    let after_clear = compute(
        &departed,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(player_entry(&after_clear).laps_completed, 12);
    assert_eq!(
        state.lock().unwrap().race_timing.cars[&0].last_in_world_laps,
        Some(12)
    );
}

#[test]
fn test_towed_car_stays_unfinished_on_results_only_pit_reentry() {
    for completed_at_crossing in [-1, 11, 12] {
        check_finish_after_pit_reentry(completed_at_crossing);
    }
}

fn check_finish_after_pit_reentry(completed_at_crossing: i32) {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        reason_out_id: Some(0),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(12, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let mut car_idx = trailing_and_leading_frame(12, 13);
    car_idx.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    car_idx.car_idx_lap_dist_pct[0] = -1.0;
    car_idx.car_idx_lap[0] = 0;
    car_idx.car_idx_laps_completed[0] = 0;
    let towed = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(player_entry(&towed).is_towed);
    assert!(!player_entry(&towed).is_finished);

    car_idx.car_idx_track_surface[0] = TrackSurface::InPitStall;
    car_idx.car_idx_lap_dist_pct[0] = 0.0;
    car_idx.car_idx_laps_completed[0] = -1;
    let reentered = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&reentered).is_towed);
    assert!(!player_entry(&reentered).is_finished);

    car_idx.car_idx_track_surface[0] = TrackSurface::OnTrack;
    car_idx.car_idx_lap_dist_pct[0] = 0.95;
    car_idx.car_idx_lap[0] = 12;
    car_idx.car_idx_laps_completed[0] = 11;
    let before_line = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&before_line).is_finished);

    car_idx.car_idx_lap_dist_pct[0] = 0.1;
    car_idx.car_idx_lap[0] = 13;
    car_idx.car_idx_laps_completed[0] = completed_at_crossing;
    let crossed = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(
        player_entry(&crossed).is_finished,
        "counter: {completed_at_crossing}"
    );
    assert_eq!(player_entry(&crossed).laps_completed, 12);

    car_idx.car_idx_lap_dist_pct[0] = 0.2;
    let after_line = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert_eq!(player_entry(&after_line).laps_completed, 12);
}

#[test]
fn advancing_results_must_exceed_the_followers_finish_baseline() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(10),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |live: &CarIdxFrame, session: &SessionSnapshot| {
        compute(
            live,
            session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        )
    };
    sample(&trailing_and_leading_frame(12, 13), &session);
    let mut live = trailing_and_leading_frame(12, 14);
    sample(&live, &session);

    live.car_idx_lap[0] = 13;
    live.car_idx_laps_completed[0] = -1;
    session.sessions[0].results_positions[0].laps_complete = Some(11);
    assert!(!player_entry(&sample(&live, &session)).is_finished);

    session.sessions[0].results_positions[0].laps_complete = Some(12);
    assert!(player_entry(&sample(&live, &session)).is_finished);
}

#[test]
fn test_delayed_results_do_not_finish_a_parked_car_without_a_live_crossing() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(12),
        reason_out_id: Some(0),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let mut in_pits = trailing_and_leading_frame(12, 14);
    in_pits.car_idx_on_pit_road[0] = true;
    in_pits.car_idx_track_surface[0] = TrackSurface::InPitStall;
    in_pits.car_idx_lap_dist_pct[0] = 0.0;
    compute(
        &in_pits,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    session.sessions[0].results_positions[0].laps_complete = Some(13);
    let mut parked = in_pits;
    parked.car_idx_on_pit_road[0] = false;
    parked.car_idx_track_surface[0] = TrackSurface::InPitStall;
    parked.car_idx_lap_dist_pct[0] = 0.0;
    parked.car_idx_lap[0] = 0;
    parked.car_idx_laps_completed[0] = -1;
    let frame = compute(
        &parked,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let player = player_entry(&frame);
    assert_eq!(player.laps_completed, 13);
    assert!(!player.is_finished);

    parked.car_idx_track_surface[0] = TrackSurface::OffTrack;
    parked.car_idx_on_pit_road[0] = true;
    let pit_road = compute(
        &parked,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&pit_road).is_finished);
}

#[test]
fn test_a_car_that_vanishes_under_the_flag_without_a_crossing_stays_towed() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    // The leader takes the flag, which arms the latch for the field behind it.
    compute(
        &trailing_and_leading_frame(12, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    // The car leaves before either source records another completed lap.
    let mut car_idx = trailing_and_leading_frame(12, 14);
    car_idx.car_idx_track_surface = vec![TrackSurface::NotInWorld, TrackSurface::OnTrack];

    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let quitter = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();

    assert!(!quitter.is_finished);
    assert!(quitter.is_towed);
}

#[test]
fn test_broadcast_checkered_bit_alone_does_not_finish_a_car() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &racing_car_idx_frame_on_lap(0, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    // The leader has taken the flag, so the sim shows the checkered bit to this
    // car too — but it is still a lap away from the line.
    let frame = compute(
        &racing_car_idx_frame_on_lap(CHECKERED, 12),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    assert!(!frame.entries[0].is_finished);
}

#[test]
fn test_a_lap_limited_race_finishes_the_leader_on_the_tick_it_turns_checkered() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &trailing_and_leading_frame(12, 13),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    // A lap-limited race enters `Checkered` on the very tick the leader crosses,
    // with no lap left for the state to wait on.
    let frame = compute(
        &trailing_and_leading_frame(12, 14),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );

    let leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    let trailing = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();

    assert!(leader.is_finished);
    assert!(!trailing.is_finished);
}

#[test]
fn test_cool_down_does_not_classify_cars_that_never_crossed_the_line() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    let frame = compute(
        &racing_car_idx_frame(0),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert!(!frame.entries[0].is_finished);

    let frame = compute(
        &racing_car_idx_frame(0),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &state,
    );

    assert!(!frame.entries[0].is_finished);
}

#[test]
fn a_verified_crossing_on_the_first_cooldown_sample_counts() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |car: &CarIdxFrame, phase, clock| {
        super::compute_with_clock(
            car,
            &session,
            &HashMap::new(),
            false,
            Some(phase),
            &state,
            Some(clock),
        )
    };
    let mut car = trailing_and_leading_frame(12, 13);
    car.car_idx_lap_dist_pct = vec![0.35, 0.98];
    sample(&car, SessionState::Racing, 10.0);
    sample(&car, SessionState::Checkered, 10.1);

    car.car_idx_lap[1] = 14;
    car.car_idx_laps_completed[1] = 13;
    car.car_idx_lap_dist_pct[1] = 0.02;
    let frame = sample(&car, SessionState::CoolDown, 10.2);
    assert!(frame.entries[0].is_finished);
    assert!(!player_entry(&frame).is_finished);
}

#[test]
fn finish_freezes_rollout_laps_and_accepts_a_matching_delayed_time() {
    let mut session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    prime_running_race(&state, &session);
    let mut car = racing_car_idx_frame_on_lap(0, 20);
    car.car_idx_last_lap_time = vec![91.2];
    car.car_idx_best_lap_time = vec![91.2];
    car.car_idx_lap_dist_pct = vec![0.98];

    let sample = |car: &CarIdxFrame, session: &SessionSnapshot, clock| {
        compute_frame(
            car,
            session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
            FrameContext {
                time: Some(clock),
                flags: None,
                freeze_motion: true,
            },
        )
    };
    sample(&car, &session, 100.0);
    car.car_idx_lap[0] = 21;
    car.car_idx_laps_completed[0] = 20;
    car.car_idx_lap_dist_pct[0] = 0.02;
    let frame = sample(&car, &session, 100.1);
    let finished = player_entry(&frame);
    assert!(finished.is_finished);
    assert_eq!(finished.last_lap_time, None);
    assert_eq!(finished.last_lap_delta, None);

    car.car_idx_lap[0] = 22;
    car.car_idx_laps_completed[0] = 21;
    car.car_idx_lap_dist_pct[0] = 0.75;
    car.car_idx_last_lap_time[0] = 78.0;
    car.car_idx_best_lap_time[0] = 78.0;
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(21),
        last_time: Some(78.0),
        ..Default::default()
    });
    let frame = sample(&car, &session, 200.0);
    let rollout = player_entry(&frame);
    assert_eq!(rollout.laps_completed, 20);
    assert_eq!(rollout.lap_dist_pct, None);
    assert_eq!(rollout.last_lap_time, None);
    assert_eq!(rollout.best_lap_time, Some(91.2));

    session.sessions[0].results_positions[0].laps_complete = Some(20);
    session.sessions[0].results_positions[0].last_time = Some(90.9);
    let frame = sample(&car, &session, 201.0);
    let updated = player_entry(&frame);
    assert_eq!(updated.last_lap_time, Some(90.9));
    assert!((updated.last_lap_delta.unwrap() + 0.3).abs() < 0.001);
}

#[test]
fn official_classification_overrides_live_ranks_and_a_previous_finish() {
    let mut session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |car: &CarIdxFrame, session: &SessionSnapshot| {
        compute(
            car,
            session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        )
    };
    sample(&trailing_and_leading_frame(12, 13), &session);
    let live = trailing_and_leading_frame(12, 14);
    assert!(sample(&live, &session).entries[0].is_finished);

    session.sessions[0].results_official = Some(true);
    session.sessions[0].results_positions = vec![
        ResultPosition {
            laps_complete: Some(11),
            ..result_at(0, 1)
        },
        ResultPosition {
            reason_out_id: Some(1),
            reason_out_str: Some("Disqualified".into()),
            laps_complete: Some(13),
            ..result_at(1, 2)
        },
    ];
    let frame = sample(&live, &session);
    assert_eq!(frame.standings_order, Some(StandingsOrder::Results));
    assert_eq!(frame.entries[0].car_idx, 0);
    assert_eq!(frame.entries[0].live_class_position, 1);
    assert!(frame.entries[0].is_finished);
    assert_eq!(frame.entries[1].race_outcome, Some(RaceOutcome::Dq));
    assert!(!frame.entries[1].is_finished);
    assert!(frame.entries[1].finish_timing.is_none());

    session.sessions[0].results_positions[1].reason_out_str = Some("Disconnected".into());
    assert_eq!(
        sample(&live, &session).entries[1].race_outcome,
        Some(RaceOutcome::Dnf)
    );
}

#[test]
fn provisional_finish_order_ignores_rollout_progress() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |car: &CarIdxFrame| {
        compute(
            car,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        )
    };
    sample(&trailing_and_leading_frame(13, 13));
    assert_eq!(
        sample(&trailing_and_leading_frame(13, 14)).entries[0].car_idx,
        1
    );

    let mut live = trailing_and_leading_frame(14, 14);
    live.car_idx_lap_dist_pct = vec![0.8, 0.05];
    let finish = sample(&live);
    assert!(finish.entries.iter().all(|entry| entry.is_finished));
    assert_eq!(finish.entries[0].car_idx, 1);

    live.car_idx_lap[0] = 15;
    live.car_idx_laps_completed[0] = 14;
    live.car_idx_track_surface[1] = TrackSurface::NotInWorld;
    live.car_idx_lap_dist_pct[1] = -1.0;
    let rollout = sample(&live);
    assert_eq!(rollout.entries[0].car_idx, 1);
    assert_eq!(rollout.entries[1].laps_completed, 13);
}

#[test]
fn caution_uses_sim_positions_and_holds_them_when_they_disappear() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = trailing_and_leading_frame(13, 12);
    let sample = |car: &CarIdxFrame, flags| {
        compute_frame(
            car,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Racing),
            &state,
            FrameContext {
                time: None,
                flags,
                freeze_motion: true,
            },
        )
    };
    assert_eq!(player_entry(&sample(&car, Some(0))).live_position, 1);
    let caution = sample(&car, Some(CAUTION));
    assert_eq!(caution.standings_order, Some(StandingsOrder::Caution));
    assert_eq!(player_entry(&caution).live_position, 2);
    car.car_idx_position.fill(0);
    let held = sample(&car, None);
    assert_eq!(held.standings_order, Some(StandingsOrder::Hold));
    assert_eq!(player_entry(&held).live_position, 2);
    assert_eq!(player_entry(&sample(&car, Some(0))).live_position, 1);
}

#[test]
fn reconnect_keeps_finishes_only_for_the_same_race() {
    let mut session = two_car_race_session();
    session.sub_session_id = Some(100);
    let mut processor = DriverEntriesProcessor::default();
    let sample =
        |processor: &DriverEntriesProcessor, session: &SessionSnapshot, car: &CarIdxFrame, time| {
            compute_with_clock(
                car,
                session,
                &HashMap::new(),
                false,
                Some(SessionState::Checkered),
                &processor.state,
                Some(time),
            )
        };
    sample(
        &processor,
        &session,
        &trailing_and_leading_frame(12, 13),
        98.0,
    );
    sample(
        &processor,
        &session,
        &trailing_and_leading_frame(12, 14),
        99.0,
    );
    processor.reset();

    let live = trailing_and_leading_frame(12, 15);
    let same_event = sample(&processor, &session, &live, 102.0);
    let finish = &same_event.entries[0];
    assert!(finish.is_finished);
    assert_eq!(finish.laps_completed, 13);
    assert!(!player_entry(&same_event).is_finished);

    processor.reset();
    session.sub_session_id = Some(101);
    let next_event = sample(&processor, &session, &live, 110.0);
    assert!(next_event.entries.iter().all(|entry| !entry.is_finished));
}

#[test]
fn cold_attach_under_checkered_waits_for_official_results() {
    let mut session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let sample = |car: &CarIdxFrame, session: &SessionSnapshot| {
        super::compute(
            car,
            session,
            &HashMap::new(),
            false,
            Some(SessionState::Checkered),
            &state,
        )
    };
    sample(&trailing_and_leading_frame(12, 14), &session);
    let rollout = trailing_and_leading_frame(12, 15);
    assert!(sample(&rollout, &session)
        .entries
        .iter()
        .all(|entry| !entry.is_finished));

    session.sessions[0].results_official = Some(true);
    session.sessions[0].results_positions = vec![ResultPosition {
        laps_complete: Some(13),
        ..result_at(1, 1)
    }];
    let frame = sample(&rollout, &session);
    assert!(frame.entries[0].is_finished);
    assert_eq!(frame.entries[0].laps_completed, 13);
}

#[test]
fn test_cool_down_does_not_classify_a_car_that_left_during_its_first_lap() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    let started = compute(
        &racing_car_idx_frame_on_lap(0, 1),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert!(!started.entries[0].is_finished);

    let departed = compute(
        &garaged_car_idx_frame(),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &state,
    );
    assert!(!departed.entries[0].is_finished);

    let mut results_session = race_session();
    results_session.sessions[0]
        .results_positions
        .push(ResultPosition {
            car_idx: 0,
            laps_complete: Some(0),
            ..Default::default()
        });
    let results_only = compute(
        &garaged_car_idx_frame(),
        &results_session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &Mutex::new(DriverEntriesState::default()),
    );
    assert!(!player_entry(&results_only).is_finished);
}

#[test]
fn test_finish_latch_does_not_apply_outside_a_race() {
    let mut session = race_session();
    session.sessions[0].session_type = SessionType::Practice;

    let state = Mutex::new(DriverEntriesState::default());

    let frame = compute(
        &racing_car_idx_frame(CHECKERED),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &state,
    );

    assert!(!frame.entries[0].is_finished);
}

#[test]
fn test_finish_latch_clears_on_the_next_session() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        &racing_car_idx_frame(0),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::CoolDown),
        &state,
    );

    let mut next_session = race_session();
    next_session.current_session_num = 1;
    next_session.sessions.push(SessionEntry {
        session_type: SessionType::Race,
        ..Default::default()
    });

    let frame = compute(
        &racing_car_idx_frame(0),
        &next_session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert!(!frame.entries[0].is_finished);
}

#[test]
fn test_garaged_car_falls_back_to_results_positions() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        position: 4,
        // Second in class, 1-indexed as the source layer delivers it.
        class_position: Some(2),
        lap: Some(0),
        time: Some(12.5),
        fastest_time: Some(91.2),
        last_time: Some(92.4),
        laps_complete: Some(17),
        reason_out_id: Some(0),
        reason_out_str: None,
    });

    let state = Mutex::new(DriverEntriesState::default());
    let frame = compute(
        &garaged_car_idx_frame(),
        &session,
        &HashMap::new(),
        false,
        None,
        &state,
    );

    let entry = &frame.entries[0];

    assert_eq!(entry.position, 4);
    assert_eq!(entry.class_position, 2);
    assert_eq!(entry.lap, 18);
    assert_eq!(entry.best_lap_time, Some(91.2));
    assert_eq!(entry.last_lap_time, Some(92.4));
    assert!(!entry.is_retired);
}

#[test]
fn off_world_live_lap_time_requires_matching_completed_lap_counts() {
    let mut session = race_session();
    let mut car_idx = garaged_car_idx_frame();
    car_idx.car_idx_last_lap_time = vec![92.4];
    car_idx.car_idx_laps_completed.clear();
    let state = Mutex::new(DriverEntriesState::default());
    let start_positions = HashMap::new();

    let frame = compute(&car_idx, &session, &start_positions, false, None, &state);
    assert_eq!(frame.entries[0].last_lap_time, None);

    session.sessions[0].results_positions = vec![ResultPosition {
        car_idx: 0,
        position: 1,
        laps_complete: Some(17),
        ..Default::default()
    }];
    car_idx.car_idx_laps_completed = vec![17];

    let frame = compute(&car_idx, &session, &start_positions, false, None, &state);
    assert_eq!(frame.entries[0].last_lap_time, Some(92.4));
}

#[test]
fn test_live_values_win_over_results_positions() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        position: 4,
        class_position: Some(1),
        lap: Some(0),
        time: Some(12.5),
        fastest_time: Some(91.2),
        last_time: Some(92.4),
        laps_complete: Some(17),
        reason_out_id: Some(0),
        reason_out_str: None,
    });

    let mut car_idx = garaged_car_idx_frame();
    car_idx.car_idx_lap_dist_pct = vec![0.3];
    car_idx.car_idx_position = vec![2];
    car_idx.car_idx_class_position = vec![1];
    car_idx.car_idx_lap = vec![19];
    car_idx.car_idx_laps_completed = vec![18];
    car_idx.car_idx_best_lap_time = vec![90.0];
    car_idx.car_idx_last_lap_time = vec![90.5];
    car_idx.car_idx_track_surface = vec![TrackSurface::OnTrack];

    let state = Mutex::new(DriverEntriesState::default());
    let frame = compute(&car_idx, &session, &HashMap::new(), false, None, &state);

    let entry = &frame.entries[0];

    assert_eq!(entry.position, 2);
    assert_eq!(entry.class_position, 1);
    assert_eq!(entry.lap, 19);
    assert_eq!(entry.best_lap_time, Some(90.0));
    assert_eq!(entry.last_lap_time, Some(90.5));
}

#[test]
fn stale_result_does_not_turn_a_restored_live_count_into_a_finish() {
    let mut session = race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_lap = vec![13];
    car.car_idx_laps_completed = vec![12];
    car.car_idx_lap_dist_pct = vec![0.5];
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    car.car_idx_laps_completed[0] = -1;
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert_eq!(
        state.lock().unwrap().race_timing.previous_laps.get(&0),
        Some(&12)
    );
    car.car_idx_laps_completed[0] = 12;
    let frame = compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&frame).is_finished);
    car.car_idx_laps_completed[0] = 13;
    let frame = compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(player_entry(&frame).is_finished);
}

#[test]
fn identical_result_times_confirm_consecutive_laps_without_live_times() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        last_time: Some(92.1),
        laps_complete: Some(2),
        ..Default::default()
    });
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_laps_completed = vec![2];
    car.car_idx_last_lap_time = vec![-1.0];
    let state = Mutex::new(DriverEntriesState::default());
    compute(&car, &session, &HashMap::new(), false, None, &state);
    car.car_idx_laps_completed[0] = 3;
    let stale = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&stale).last_lap_delta, None);
    session.sessions[0].results_positions[0].laps_complete = Some(3);
    let frame = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_delta, Some(0.0));
}

#[test]
fn off_world_samples_keep_the_delta_and_late_lap_count() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(5),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_laps_completed = vec![5];
    car.car_idx_last_lap_time = vec![92.0];
    compute(&car, &session, &HashMap::new(), false, None, &state);
    car.car_idx_laps_completed[0] = 6;
    car.car_idx_last_lap_time[0] = 91.0;
    assert_eq!(
        player_entry(&compute(
            &car,
            &session,
            &HashMap::new(),
            false,
            None,
            &state
        ))
        .last_lap_delta,
        Some(-1.0)
    );
    car.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    car.car_idx_last_lap_time[0] = -1.0;
    let departed = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&departed).last_lap_delta, Some(-1.0));
    car.car_idx_laps_completed[0] = 7;
    let late = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&late).laps_completed, 7);
    assert_eq!(player_entry(&late).last_lap_delta, None);
    car.car_idx_laps_completed[0] = 0;
    let cleared = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&cleared).laps_completed, 7);
}

#[test]
fn fresh_result_replaces_stale_live_lap_time() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(2),
        last_time: Some(92.0),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_laps_completed = vec![2];
    car.car_idx_last_lap_time = vec![92.0];
    compute(&car, &session, &HashMap::new(), false, None, &state);
    car.car_idx_laps_completed[0] = 3;
    session.sessions[0].results_positions[0].laps_complete = Some(3);
    session.sessions[0].results_positions[0].last_time = Some(91.0);
    let frame = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_time, Some(91.0));
    assert_eq!(player_entry(&frame).last_lap_delta, Some(-1.0));
    let unchanged = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&unchanged).last_lap_time, Some(91.0));
    assert_eq!(player_entry(&unchanged).last_lap_delta, Some(-1.0));
}

#[test]
fn delayed_result_replaces_stale_live_lap_time() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(2),
        last_time: Some(92.0),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_laps_completed = vec![2];
    car.car_idx_last_lap_time = vec![92.0];
    compute(&car, &session, &HashMap::new(), false, None, &state);
    car.car_idx_laps_completed[0] = 3;
    let waiting = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&waiting).last_lap_time, None);
    assert_eq!(player_entry(&waiting).last_lap_delta, None);
    session.sessions[0].results_positions[0].laps_complete = Some(3);
    session.sessions[0].results_positions[0].last_time = Some(91.0);
    let frame = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_time, Some(91.0));
    assert_eq!(player_entry(&frame).last_lap_delta, Some(-1.0));
}

#[test]
fn late_off_world_lap_does_not_finish_a_towed_car() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut frame = trailing_and_leading_frame(7, 13);
    compute(
        &frame,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    frame.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    frame.car_idx_lap_dist_pct[0] = -1.0;
    compute(
        &frame,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    frame.car_idx_laps_completed[0] = 7;
    frame.car_idx_lap[1] = 14;
    frame.car_idx_laps_completed[1] = 13;
    let at_flag = compute(
        &frame,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert_eq!(player_entry(&at_flag).laps_completed, 7);
    assert!(!player_entry(&at_flag).is_finished);
    assert!(player_entry(&at_flag).is_towed);
    let later = compute(
        &frame,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&later).is_finished);
    assert!(player_entry(&later).is_towed);
    frame.car_idx_track_surface[0] = TrackSurface::OnTrack;
    frame.car_idx_lap_dist_pct[0] = 0.2;
    frame.car_idx_lap[0] = 7;
    frame.car_idx_laps_completed[0] = -1;
    let returned = compute(
        &frame,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert_eq!(player_entry(&returned).laps_completed, 6);
    assert_eq!(player_entry(&returned).lap, 7);
}

#[test]
fn leader_wrap_under_checkered_finishes_without_a_live_lap_counter() {
    let mut session = race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(4),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = garaged_car_idx_frame();
    car.car_idx_track_surface = vec![TrackSurface::OnTrack];
    car.car_idx_lap = vec![5];
    car.car_idx_laps_completed = vec![-1];
    car.car_idx_lap_dist_pct = vec![0.9];
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    car.car_idx_lap[0] = 6;
    car.car_idx_lap_dist_pct[0] = 0.1;
    let frame = compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(player_entry(&frame).is_finished);
}

#[test]
fn pre_flag_wrap_without_counter_is_not_a_late_finish() {
    let mut session = two_car_race_session();
    session.sessions[0].results_positions.push(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = trailing_and_leading_frame(12, 13);
    car.car_idx_laps_completed[0] = -1;
    car.car_idx_lap_dist_pct[0] = 0.9;
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    car.car_idx_lap[0] = 13;
    car.car_idx_lap_dist_pct[0] = 0.1;
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert_eq!(
        state.lock().unwrap().race_timing.previous_laps.get(&0),
        Some(&12)
    );
    let mut leader_cross = trailing_and_leading_frame(13, 14);
    leader_cross.car_idx_laps_completed[0] = -1;
    leader_cross.car_idx_lap_dist_pct[0] = 0.2;
    compute(
        &leader_cross,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    session.sessions[0].results_positions[0].laps_complete = Some(12);
    let frame = compute(
        &leader_cross,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&frame).is_finished);
}

#[test]
fn identical_pit_lane_lap_needs_a_result_for_the_new_completed_lap() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        last_time: Some(92.1),
        laps_complete: Some(2),
        ..Default::default()
    });
    let mut car = garaged_car_idx_frame();
    car.car_idx_lap = vec![3];
    car.car_idx_laps_completed = vec![2];
    car.car_idx_lap_dist_pct = vec![0.02];
    car.car_idx_on_pit_road = vec![true];
    car.car_idx_track_surface = vec![TrackSurface::OffTrack];
    car.car_idx_last_lap_time = vec![92.1];
    let state = Mutex::new(DriverEntriesState::default());
    let start_positions = HashMap::new();

    compute(&car, &session, &start_positions, false, None, &state);
    car.car_idx_lap[0] = 4;
    car.car_idx_laps_completed[0] = 3;
    let frame = compute(&car, &session, &start_positions, false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_delta, None);

    session.sessions[0].results_positions[0].laps_complete = Some(3);
    let frame = compute(&car, &session, &start_positions, false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_delta, Some(0.0));
}

#[test]
fn lap_delta_waits_for_the_new_time_and_survives_a_stale_counter() {
    let mut session = session_with_result(ResultPosition {
        car_idx: 0,
        lap: Some(1),
        ..Default::default()
    });
    let mut car = garaged_car_idx_frame();
    car.car_idx_lap_dist_pct = vec![0.02];
    let state = Mutex::new(DriverEntriesState::default());

    {
        let mut observe = |laps, time, surface| {
            car.car_idx_lap = vec![laps + 1];
            car.car_idx_laps_completed = vec![laps];
            car.car_idx_last_lap_time = vec![time];
            car.car_idx_track_surface = vec![surface];
            let frame = compute(&car, &session, &HashMap::new(), false, None, &state);
            player_entry(&frame).last_lap_delta
        };

        let track = TrackSurface::OnTrack;
        assert_eq!(observe(5, 124.75, track), None);
        assert_eq!(observe(6, 124.75, track), None);
        assert_eq!(observe(6, 122.75, track), Some(-2.0));
        assert_eq!(observe(5, 124.75, track), None);
        assert_eq!(observe(6, 122.75, track), Some(-2.0));
        assert_eq!(observe(0, -1.0, TrackSurface::NotInWorld), Some(-2.0));
        assert_eq!(observe(0, -1.0, track), None);
        assert_eq!(observe(1, 95.0, track), None);
        assert_eq!(observe(2, 94.0, track), Some(-1.0));
    }

    session.current_session_num += 1;
    let frame = compute(&car, &session, &HashMap::new(), false, None, &state);
    assert_eq!(player_entry(&frame).last_lap_delta, None);
}

#[test]
fn leader_without_results_or_completed_counter_finishes_on_wrap() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = racing_car_idx_frame_on_lap(0, 5);
    car.car_idx_laps_completed = vec![-1];
    car.car_idx_lap_dist_pct = vec![0.9];
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    let stationary = compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&stationary).is_finished);
    car.car_idx_lap[0] = 6;
    car.car_idx_lap_dist_pct[0] = 0.1;
    let finished = compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(player_entry(&finished).is_finished);
}

#[test]
fn trailing_car_without_completed_counter_finishes_on_wrap_after_flag() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut car = trailing_and_leading_frame(12, 13);
    car.car_idx_laps_completed[0] = -1;
    car.car_idx_lap_dist_pct[0] = 0.9;
    compute(
        &car,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    let mut leader_cross = trailing_and_leading_frame(12, 14);
    leader_cross.car_idx_laps_completed[0] = -1;
    leader_cross.car_idx_lap_dist_pct[0] = 0.95;
    let flagged = compute(
        &leader_cross,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(!player_entry(&flagged).is_finished);
    leader_cross.car_idx_lap[0] = 13;
    leader_cross.car_idx_lap_dist_pct[0] = 0.05;
    let finished = compute(
        &leader_cross,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Checkered),
        &state,
    );
    assert!(player_entry(&finished).is_finished);
}

#[test]
fn lap_delta_keeps_an_accepted_time_through_missing_samples() {
    let mut history = LapHistory::default();
    assert_eq!(history.observe(7, Some(89.0), false, true), None);
    assert_eq!(history.observe(8, Some(90.0), false, true), Some(1.0));
    assert_eq!(history.observe(8, None, false, true), Some(1.0));
    assert_eq!(history.observe(8, Some(89.0), false, true), Some(1.0));
    assert_eq!(history.observe(9, Some(91.0), false, true), Some(1.0));
}

#[test]
fn accepts_fresh_lap_times_after_wraps_without_a_completed_counter() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut live = racing_car_idx_frame_on_lap(0, 8);
    live.car_idx_laps_completed.clear();

    let mut sample = |lap, progress, time| {
        live.car_idx_lap[0] = lap;
        live.car_idx_lap_dist_pct[0] = progress;
        live.car_idx_last_lap_time[0] = time;
        let frame = compute(
            &live,
            &session,
            &HashMap::new(),
            false,
            Some(SessionState::Racing),
            &state,
        );
        let entry = player_entry(&frame);
        (
            entry.laps_completed,
            entry.last_lap_time,
            entry.last_lap_delta,
        )
    };

    assert_eq!(sample(8, 0.95, 89.0), (7, None, None));
    assert_eq!(sample(9, 0.01, 89.0), (8, None, None));
    assert_eq!(sample(9, 0.02, 90.0), (8, Some(90.0), None));
    assert_eq!(sample(9, 0.95, 90.0), (8, Some(90.0), None));
    assert_eq!(sample(10, 0.01, 91.0), (9, Some(91.0), Some(1.0)));
    assert_eq!(sample(12, 0.2, 92.0), (11, None, None));
}

#[test]
fn lap_delta_needs_adjacent_timed_laps_after_a_penalty_or_a_skipped_frame() {
    let mut history = LapHistory::default();
    assert_eq!(history.observe(3, Some(49.5), false, true), None);
    assert_eq!(history.observe(4, Some(49.25), false, true), Some(-0.25));
    assert_eq!(history.observe(5, Some(49.25), false, true), None);
    assert_eq!(history.observe(5, None, false, true), None);
    assert_eq!(history.observe(6, Some(143.25), false, true), None);
    assert_eq!(history.observe(7, Some(50.0), false, true), Some(-93.25));
    assert_eq!(history.observe(9, Some(89.0), false, true), None);
    assert_eq!(history.observe(9, Some(88.0), false, true), None);
    assert_eq!(history.observe(10, Some(90.0), false, true), Some(2.0));
}

#[test]
fn omits_a_result_lap_time_when_the_retained_count_is_newer() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(11),
        last_time: Some(92.5),
        ..Default::default()
    });
    let state = Mutex::new(DriverEntriesState::default());
    let mut live = garaged_car_idx_frame();
    live.car_idx_track_surface = vec![TrackSurface::OnTrack];
    live.car_idx_lap = vec![13];
    live.car_idx_laps_completed = vec![12];
    live.car_idx_lap_dist_pct = vec![0.4];
    live.car_idx_last_lap_time = vec![93.1];
    compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    live.car_idx_track_surface[0] = TrackSurface::NotInWorld;
    live.car_idx_lap_dist_pct[0] = -1.0;
    live.car_idx_lap[0] = 0;
    live.car_idx_laps_completed[0] = 11;
    live.car_idx_last_lap_time[0] = 93.1;
    let departed = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(player_entry(&departed).laps_completed, 12);
    assert_eq!(player_entry(&departed).last_lap_time, None);
}

#[test]
fn retains_a_matching_results_lap_time_for_in_world_entries() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        laps_complete: Some(12),
        last_time: Some(92.5),
        ..Default::default()
    });
    let mut live = garaged_car_idx_frame();
    live.car_idx_track_surface = vec![TrackSurface::OnTrack];
    live.car_idx_lap = vec![13];
    live.car_idx_laps_completed = vec![-1];
    live.car_idx_lap_dist_pct = vec![0.4];
    live.car_idx_last_lap_time = vec![-1.0];

    let frame = compute(
        &live,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &Mutex::new(DriverEntriesState::default()),
    );

    assert_eq!(player_entry(&frame).last_lap_time, Some(92.5));
}

#[test]
fn f2_zero_belongs_to_the_live_leader_when_official_order_differs() {
    let session = two_car_race_session();
    let state = Mutex::new(DriverEntriesState::default());
    let mut car_idx = two_car_frame(TrackSurface::OnTrack, 0.6, 0.5);
    car_idx.car_idx_position = vec![1, 2];
    car_idx.car_idx_f2_time = vec![12.0, 0.0];

    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    let live_leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .unwrap();
    let official_leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();

    assert_eq!(live_leader.position, 2);
    assert_eq!(live_leader.live_position, 1);
    assert_eq!(live_leader.f2_time, Some(0.0));
    assert_eq!(official_leader.position, 1);
    assert_eq!(official_leader.live_position, 2);
    assert_eq!(official_leader.f2_time, Some(12.0));

    car_idx.car_idx_f2_time[0] = 0.0;
    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    let non_leader = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 0)
        .unwrap();
    assert_eq!(non_leader.live_position, 2);
    assert_eq!(non_leader.f2_time, None);

    car_idx.car_idx_f2_time.clear();
    let frame = compute(
        &car_idx,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );
    assert!(frame.entries.iter().all(|entry| entry.f2_time.is_none()));
}

#[test]
fn test_non_zero_reason_out_marks_entry_retired() {
    let session = session_with_result(ResultPosition {
        car_idx: 0,
        position: 12,
        class_position: Some(5),
        lap: None,
        time: None,
        fastest_time: Some(95.0),
        last_time: Some(96.0),
        laps_complete: Some(3),
        reason_out_id: Some(2),
        reason_out_str: None,
    });

    let state = Mutex::new(DriverEntriesState::default());
    let frame = compute(
        &garaged_car_idx_frame(),
        &session,
        &HashMap::new(),
        false,
        None,
        &state,
    );

    assert!(frame.entries[0].is_retired);
}

#[test]
fn test_parse_start_positions_from_qualify_maps_car_idx_to_positions() {
    let entries = vec![
        QualifyResultEntry {
            car_idx: 0,
            position: 1,
            class_position: Some(1),
            ..Default::default()
        },
        QualifyResultEntry {
            car_idx: 5,
            position: 2,
            class_position: Some(2),
            ..Default::default()
        },
    ];

    let map = parse_start_positions_from_qualify(&entries);

    assert_eq!(map.get(&0), Some(&(1, 1)));
    assert_eq!(map.get(&5), Some(&(2, 2)));
}

#[test]
fn test_parse_start_positions_from_qualify_fallback_class_pos() {
    let entries = vec![QualifyResultEntry {
        car_idx: 3,
        position: 3,
        class_position: None,
        ..Default::default()
    }];

    let map = parse_start_positions_from_qualify(&entries);

    // With no class position the overall one stands in for it.
    assert_eq!(map.get(&3), Some(&(3, 3)));
}

#[test]
fn test_parse_start_positions_prefers_qualify_over_results() {
    // This test validates the priority rule exercised in runtime.rs:
    // QualifyResultsInfo (non-empty) wins over ResultsPositions, because
    // ResultsPositions reflects CURRENT race order, not the starting grid.
    let results = vec![ResultPosition {
        car_idx: 1,
        position: 2,
        class_position: Some(2),
        lap: None,
        time: None,
        fastest_time: None,
        last_time: None,
        laps_complete: None,
        reason_out_id: None,
        reason_out_str: None,
    }];

    let qualify = vec![QualifyResultEntry {
        car_idx: 1,
        position: 1, // pole — different from where the results have it
        class_position: Some(1),
        ..Default::default()
    }];

    let from_results = parse_start_positions(&results);
    let from_qualify = parse_start_positions_from_qualify(&qualify);

    // Runtime picks from_qualify when non-empty
    assert_eq!(from_qualify.get(&1), Some(&(1, 1)));
    assert_eq!(from_results.get(&1), Some(&(2, 2)));
    // Confirm they differ — so the selection matters
    assert_ne!(from_qualify.get(&1), from_results.get(&1));
}

#[test]
fn test_parse_start_positions_from_qualify_empty() {
    let map = parse_start_positions_from_qualify(&[]);
    assert!(map.is_empty());
}

fn apply_pit_machine(
    pit_states: &mut HashMap<i32, PitState>,
    car_idx: i32,
    on_pit_road: bool,
    track_surface: TrackSurface,
) -> PitState {
    let prev = pit_states.get(&car_idx).copied().unwrap_or(PitState::None);
    let next = next_pit_state(prev, on_pit_road, track_surface);
    pit_states.insert(car_idx, next);
    next
}

#[test]
fn test_pit_state_none_when_not_on_pit_road() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    let result = apply_pit_machine(&mut states, 1, false, TrackSurface::OnTrack);
    assert_eq!(result, PitState::None);
}

#[test]
fn test_pit_state_in_when_entering_pit_road() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    let result = apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    assert_eq!(result, PitState::In);
}

#[test]
fn test_pit_state_transitions_to_stall() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    let result = apply_pit_machine(&mut states, 1, true, TrackSurface::InPitStall);
    assert_eq!(result, PitState::Stall);
}

#[test]
fn test_pit_state_transitions_to_exit_after_stall() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    apply_pit_machine(&mut states, 1, true, TrackSurface::InPitStall);
    let result = apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    assert_eq!(result, PitState::Exit);
}

#[test]
fn test_pit_state_resets_to_none_when_leaving_pit_road() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    apply_pit_machine(&mut states, 1, true, TrackSurface::InPitStall);
    apply_pit_machine(&mut states, 1, true, TrackSurface::AproachingPits);
    let result = apply_pit_machine(&mut states, 1, false, TrackSurface::OnTrack);
    assert_eq!(result, PitState::None);
}

#[test]
fn test_pit_state_stall_directly_if_entering_already_in_stall() {
    let mut states: HashMap<i32, PitState> = HashMap::new();
    let result = apply_pit_machine(&mut states, 1, true, TrackSurface::InPitStall);
    assert_eq!(result, PitState::Stall);
}

pub(crate) fn make_live_entry(
    car_idx: i32,
    position: i32,
    lap: i32,
    lap_dist_pct: f32,
    track_surface: TrackSurface,
) -> DriverEntry {
    DriverEntry {
        car_idx,
        car_class_id: 0,
        tire_compound: String::new(),
        position,
        class_position: position,
        live_position: 0,
        live_class_position: 0,
        start_pos_overall: Some(position),
        start_pos_class: Some(position),
        laps_completed: lap.saturating_sub(1),
        lap,
        lap_dist_pct: (lap_dist_pct >= 0.0).then_some(lap_dist_pct),
        last_lap_time: None,
        last_lap_delta: None,
        best_lap_time: None,
        qualify_time: None,
        f2_time: None,
        est_time: 0.0,
        track_surface,
        i_rating: 0,
        is_player: false,
        on_pit_road: false,
        estimated_ir_delta_live: None,
        estimated_ir_delta_official: None,
        player_ir_delta_live: None,
        player_ir_delta_official: None,
        relative_lap_dist: Some(0.0),
        class_est_lap_time: 0.0,
        raw_flags: 0,
        results_position_lap: None,
        results_position_time: None,
        finish_timing: None,
        race_outcome: None,
        is_retired: false,
        is_finished: false,
        is_towed: false,
        pit_state: PitState::None,
        speed: None,
        pit_lap: None,
        pit_elapsed_s: None,
        last_pit_s: None,
    }
}

fn make_ir_entry(
    car_idx: i32,
    class_position: i32,
    live_class_position: i32,
    i_rating: i32,
) -> DriverEntry {
    let mut entry = make_live_entry(car_idx, class_position, 0, 0.0, TrackSurface::OnTrack);

    entry.class_position = class_position;
    entry.live_class_position = live_class_position;
    entry.i_rating = i_rating;

    entry
}

#[test]
fn test_static_car_fields_stay_off_the_wire() {
    // The frontend joins these on from `SessionSnapshot.cars`; sending them
    // again would put them back on every 10 Hz tick.
    let mut entry = make_live_entry(3, 1, 0, 0.5, TrackSurface::OnTrack);
    entry.car_class_id = 4029;
    entry.i_rating = 2500;

    let json = serde_json::to_value(&entry).unwrap();

    assert!(json.get("carIdx").is_some());
    assert!(json.get("carClassId").is_none());
    assert!(json.get("iRating").is_none());
}

#[test]
fn test_ir_deltas_rank_densely_around_gaps() {
    // Positions numbered over a wider field leave gaps; the field still has to
    // score as three cars ranked 1..3.
    let sparse = vec![
        make_ir_entry(0, 2, 2, 3000),
        make_ir_entry(1, 5, 5, 2000),
        make_ir_entry(2, 9, 9, 1000),
    ];

    let dense = vec![
        make_ir_entry(0, 1, 1, 3000),
        make_ir_entry(1, 2, 2, 2000),
        make_ir_entry(2, 3, 3, 1000),
    ];

    assert_eq!(
        compute_ir_deltas(&sparse, true),
        compute_ir_deltas(&dense, true)
    );
}

#[test]
fn test_ir_deltas_score_an_unplaced_car_as_a_non_starter() {
    // Car 9 sits in the garage: the sim never placed it. It is registered for the
    // race, so it is scored as a non-starter — it loses, and its rating stays in
    // the field the others are scored against. Its live position is ignored.
    let with_non_starter = vec![
        make_ir_entry(0, 1, 1, 3000),
        make_ir_entry(1, 2, 2, 2000),
        make_ir_entry(9, 0, 3, 1500),
        make_ir_entry(2, 3, 4, 1000),
    ];

    let starters_only = vec![
        make_ir_entry(0, 1, 1, 3000),
        make_ir_entry(1, 2, 2, 2000),
        make_ir_entry(2, 3, 3, 1000),
    ];

    let deltas = compute_ir_deltas(&with_non_starter, true);

    assert!(deltas[&9].driver < 0);
    assert_ne!(deltas, compute_ir_deltas(&starters_only, true));
}

#[test]
fn test_ir_deltas_need_two_starters_in_the_class() {
    // Before anyone is placed there is no race to score, non-starters included.
    let entries = vec![make_ir_entry(0, 1, 1, 2000), make_ir_entry(1, 0, 2, 2000)];

    assert!(compute_ir_deltas(&entries, true).is_empty());
}

#[test]
fn test_ir_deltas_skip_unrated_cars() {
    // An AI car carries no iRating: it is nobody to score against.
    let entries = vec![
        make_ir_entry(0, 1, 1, 2000),
        make_ir_entry(1, 2, 2, 0),
        make_ir_entry(2, 3, 3, 2000),
    ];

    let deltas = compute_ir_deltas(&entries, true);

    assert_eq!(deltas.get(&1), None);
    assert!(deltas[&0].driver > 0);
    assert!(deltas[&2].driver < 0);
}

#[test]
fn test_ir_deltas_follow_live_order_mid_lap() {
    // Official positions still show the pre-overtake order; on track car 1 is ahead.
    // Equal iRating, so the delta is symmetric and the live leader is the one gaining.
    let entries = vec![make_ir_entry(0, 1, 2, 2000), make_ir_entry(1, 2, 1, 2000)];

    let deltas = compute_ir_deltas(&entries, true);

    assert!(deltas[&1].driver > 0);
    assert!(deltas[&0].driver < 0);
}

#[test]
fn test_ir_deltas_follow_official_order_when_not_live() {
    // Same mid-overtake frame as above. Scored on the official order instead, the
    // car still officially leading is the one gaining — the mirror of the live answer.
    let entries = vec![make_ir_entry(0, 1, 2, 2000), make_ir_entry(1, 2, 1, 2000)];

    let deltas = compute_ir_deltas(&entries, false);

    assert!(deltas[&0].driver > 0);
    assert!(deltas[&1].driver < 0);
}

#[test]
fn test_player_ir_deltas_predict_their_finish_at_each_rank_in_their_class() {
    let mut entries = vec![
        make_ir_entry(0, 1, 2, 3100),
        make_ir_entry(1, 2, 3, 2250),
        make_ir_entry(2, 3, 1, 1200),
        make_ir_entry(3, 4, 4, 1800),
        make_ir_entry(4, 5, 5, 2000),
        make_ir_entry(5, 0, 6, 1750),
        make_ir_entry(6, 1, 1, 2900),
        make_ir_entry(7, 2, 2, 1650),
        make_ir_entry(8, 6, 7, 0),
    ];
    entries[1].is_player = true;
    entries[6].car_class_id = 1;
    entries[7].car_class_id = 1;

    for use_live in [true, false] {
        let deltas = compute_ir_deltas(&entries, use_live);

        for (index, entry) in entries.iter().enumerate().take(5) {
            let mut reordered = entries.clone();
            reordered[1].class_position = entry.class_position;
            reordered[1].live_class_position = entry.live_class_position;
            reordered[index].class_position = entries[1].class_position;
            reordered[index].live_class_position = entries[1].live_class_position;

            assert_eq!(
                deltas[&entry.car_idx].player,
                Some(compute_ir_deltas(&reordered, use_live)[&1].driver)
            );
        }

        for car_idx in [5, 6, 7] {
            assert_eq!(deltas[&car_idx].player, None);
        }

        assert!(!deltas.contains_key(&8));

        let mut player_unplaced = entries.clone();
        player_unplaced[1].class_position = 0;
        assert!(compute_ir_deltas(&player_unplaced, use_live)
            .values()
            .all(|delta| delta.player.is_none()));
    }
}

#[test]
fn test_ranking_mode_follows_the_session() {
    assert_eq!(
        resolve_ranking_mode(true, Some(SessionState::Racing)),
        RankingMode::TrackOrder
    );
    assert_eq!(
        resolve_ranking_mode(true, Some(SessionState::ParadeLaps)),
        RankingMode::Grid
    );
    assert_eq!(
        resolve_ranking_mode(true, Some(SessionState::GetInCar)),
        RankingMode::Grid
    );
    assert_eq!(resolve_ranking_mode(true, None), RankingMode::Grid);
    assert_eq!(
        resolve_ranking_mode(false, Some(SessionState::Racing)),
        RankingMode::Official
    );
}

#[test]
fn test_grid_order_ignores_where_the_cars_are_parked() {
    // On the grid the field straddles the start/finish line, so the car sitting
    // at 0.99 has covered a whole lap more than the one at 0.01 as far as lap
    // progress is concerned. Only the qualifying slot means anything here.
    let mut entries = vec![
        make_live_entry(7, 3, 0, 0.99, TrackSurface::OnTrack),
        make_live_entry(2, 1, 0, 0.01, TrackSurface::OnTrack),
        make_live_entry(5, 2, 0, 0.995, TrackSurface::OnTrack),
    ];

    entries[0].start_pos_overall = Some(3);
    entries[1].start_pos_overall = Some(1);
    entries[2].start_pos_overall = Some(2);

    assign_static_positions(&mut entries, grid_sort_key);

    assert_eq!(entries[0].car_idx, 2);
    assert_eq!(entries[1].car_idx, 5);
    assert_eq!(entries[2].car_idx, 7);
    assert_eq!(entries[0].live_position, 1);
}

#[test]
fn test_cars_without_a_grid_slot_line_up_behind_the_grid() {
    // Joined after qualifying: no slot, so nothing to place it by except the
    // official position — and never ahead of a car that actually qualified.
    let mut entries = vec![
        make_live_entry(4, 9, 0, 0.5, TrackSurface::OnTrack),
        make_live_entry(1, 2, 0, 0.5, TrackSurface::OnTrack),
    ];

    entries[0].start_pos_overall = None;
    entries[1].start_pos_overall = Some(20);

    assign_static_positions(&mut entries, grid_sort_key);

    assert_eq!(entries[0].car_idx, 1);
    assert_eq!(entries[1].car_idx, 4);
}

#[test]
fn test_qualifying_ranks_by_the_official_order_not_the_track() {
    // Lone qualify: the sim ranks by best lap, the car half a lap up the road
    // has not beaten it.
    let mut entries = vec![
        make_live_entry(3, 2, 4, 0.90, TrackSurface::OnTrack),
        make_live_entry(8, 1, 4, 0.10, TrackSurface::OnTrack),
    ];

    assign_static_positions(&mut entries, official_sort_key);

    assert_eq!(entries[0].car_idx, 8);
    assert_eq!(entries[0].live_position, 1);
    assert_eq!(entries[1].car_idx, 3);
}

#[test]
fn test_qualify_time_comes_from_the_qualify_results() {
    let mut session = race_session();

    session.qualify_results = vec![QualifyResultEntry {
        car_idx: 0,
        position: 0,
        class_position: Some(0),
        fastest_time: Some(88.5),
        fastest_lap: Some(3),
    }];

    let state = Mutex::new(DriverEntriesState::default());
    let frame = compute(
        &racing_car_idx_frame(0),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(frame.entries[0].qualify_time, Some(88.5));
}

#[test]
fn test_qualify_time_is_absent_without_a_qualifying_lap() {
    let session = race_session();
    let state = Mutex::new(DriverEntriesState::default());

    let frame = compute(
        &racing_car_idx_frame(0),
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::Racing),
        &state,
    );

    assert_eq!(frame.entries[0].qualify_time, None);
}

#[test]
fn test_live_positions_ignore_official_order() {
    // Practice/qualifying: official position ranks by best lap, live ranks by
    // track order. Both are kept; the frontend decides which one to show.
    let mut entries = vec![
        make_live_entry(0, 2, 3, 0.90, TrackSurface::OnTrack),
        make_live_entry(1, 1, 3, 0.10, TrackSurface::OnTrack),
    ];

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 0);
    assert_eq!(entries[0].live_position, 1);
    assert_eq!(entries[0].position, 2);
    assert_eq!(entries[1].live_position, 2);
    assert_eq!(entries[1].position, 1);
}

#[test]
fn test_live_positions_reflect_mid_lap_overtake() {
    // Car 0 is officially P2 but is ahead on track — the pass has not reached
    // the start/finish line yet, so CarIdxPosition still shows the old order.
    let mut entries = vec![
        make_live_entry(1, 1, 3, 0.10, TrackSurface::OnTrack),
        make_live_entry(0, 2, 3, 0.90, TrackSurface::OnTrack),
    ];

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 0);
    assert_eq!(entries[0].live_position, 1);
    assert_eq!(entries[1].car_idx, 1);
    assert_eq!(entries[1].live_position, 2);
}

#[test]
fn test_live_positions_rank_lapped_cars_behind() {
    let mut entries = vec![
        make_live_entry(0, 1, 2, 0.05, TrackSurface::OnTrack),
        make_live_entry(1, 2, 1, 0.95, TrackSurface::OnTrack),
    ];

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 0);
    assert_eq!(entries[1].car_idx, 1);
}

#[test]
fn test_cars_out_of_world_slot_in_by_official_position() {
    let mut entries = vec![
        make_live_entry(0, 3, 5, 0.50, TrackSurface::NotInWorld),
        make_live_entry(1, 2, 1, 0.10, TrackSurface::OnTrack),
        make_live_entry(2, 4, 1, 0.05, TrackSurface::OnTrack),
    ];

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 1);
    assert_eq!(entries[1].car_idx, 0);
    assert_eq!(entries[1].live_position, 2);
    assert_eq!(entries[2].car_idx, 2);
}

#[test]
fn test_live_class_positions_count_within_class() {
    let mut entries = vec![
        make_live_entry(0, 1, 3, 0.90, TrackSurface::OnTrack),
        make_live_entry(1, 2, 3, 0.80, TrackSurface::OnTrack),
        make_live_entry(2, 3, 3, 0.70, TrackSurface::OnTrack),
    ];

    entries[1].car_class_id = 7;

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].live_class_position, 1);
    assert_eq!(entries[1].live_class_position, 1);
    assert_eq!(entries[2].live_class_position, 2);
}

#[test]
fn test_pitting_car_holds_its_official_position() {
    // Captured from a live race: the car in its box is far along the lap, but
    // its official position is well down the order. Track distance must not
    // promote it over the cars still racing.
    let mut entries = vec![
        make_live_entry(9, 31, 5, 0.99, TrackSurface::InPitStall),
        make_live_entry(4, 25, 5, 0.507, TrackSurface::OnTrack),
        make_live_entry(28, 26, 5, 0.503, TrackSurface::OnTrack),
        make_live_entry(7, 30, 5, 0.490, TrackSurface::OnTrack),
    ];

    entries[0].on_pit_road = true;

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 4);
    assert_eq!(entries[1].car_idx, 28);
    assert_eq!(entries[2].car_idx, 7);
    assert_eq!(entries[3].car_idx, 9);
    assert_eq!(entries[3].live_position, 4);
}

#[test]
fn test_car_in_the_garage_keeps_its_official_position() {
    // The leader being towed must not fall to the bottom of the table.
    let mut entries = vec![
        make_live_entry(11, 1, -1, -1.0, TrackSurface::NotInWorld),
        make_live_entry(37, 2, 6, 0.208, TrackSurface::OnTrack),
        make_live_entry(23, 3, 6, 0.188, TrackSurface::OnTrack),
    ];

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 11);
    assert_eq!(entries[0].live_position, 1);
    assert_eq!(entries[1].car_idx, 37);
    assert_eq!(entries[2].car_idx, 23);
}

#[test]
fn test_non_racing_cars_keep_official_order_among_themselves() {
    let mut entries = vec![
        make_live_entry(1, 5, -1, -1.0, TrackSurface::NotInWorld),
        make_live_entry(2, 3, 4, 0.80, TrackSurface::InPitStall),
        make_live_entry(3, 1, 6, 0.10, TrackSurface::OnTrack),
    ];

    entries[1].on_pit_road = true;

    assign_race_positions(&mut entries, &RaceTiming::default());

    assert_eq!(entries[0].car_idx, 3);
    assert_eq!(entries[1].car_idx, 2);
    assert_eq!(entries[2].car_idx, 1);
}

fn result_at(car_idx: i32, position: i32) -> ResultPosition {
    ResultPosition {
        car_idx,
        position,
        class_position: Some(position),
        lap: Some(0),
        time: Some(0.0),
        fastest_time: None,
        last_time: None,
        laps_complete: Some(0),
        reason_out_id: Some(0),
        reason_out_str: None,
    }
}

/// Shaped like the capture: the player (car 0) sits in the pit stall carrying a
/// junk live position of 1, cars 1 and 2 are on track with live positions zeroed,
/// and `ResultsPositions` ranks the player last.
fn unpublished_positions_session(session_type: SessionType) -> SessionSnapshot {
    SessionSnapshot {
        player_car_idx: 0,
        current_session_num: 0,
        is_official: true,
        cars: (0..3)
            .map(|car_idx| CarEntry {
                car_idx,
                i_rating: 2000,
                ..Default::default()
            })
            .collect(),
        sessions: vec![SessionEntry {
            session_type,
            results_positions: vec![result_at(1, 1), result_at(2, 2), result_at(0, 3)],
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn unpublished_positions_frame() -> CarIdxFrame {
    CarIdxFrame {
        car_idx_lap_dist_pct: vec![0.02, 0.60, 0.40],
        car_idx_on_pit_road: vec![true, false, false],
        car_idx_position: vec![1, 0, 0],
        car_idx_class_position: vec![1, 0, 0],
        car_idx_lap: vec![1, 15, 15],
        car_idx_laps_completed: vec![0, 14, 14],
        car_idx_last_lap_time: vec![-1.0, -1.0, -1.0],
        car_idx_best_lap_time: vec![-1.0, -1.0, -1.0],
        car_idx_f2_time: vec![0.0, 0.0, 0.0],
        car_idx_est_time: vec![0.0, 0.0, 0.0],
        car_idx_track_surface: vec![
            TrackSurface::InPitStall,
            TrackSurface::OnTrack,
            TrackSurface::OnTrack,
        ],
        car_idx_tire_compound: vec![-1, -1, -1],
        car_idx_session_flags: vec![0, 0, 0],
        spotter: None,
    }
}

fn compute_unpublished(
    car_idx: &CarIdxFrame,
    session: &SessionSnapshot,
    session_state: SessionState,
) -> DriverEntriesFrame {
    let state = Mutex::new(DriverEntriesState::default());

    compute(
        car_idx,
        session,
        &HashMap::new(),
        true,
        Some(session_state),
        &state,
    )
}

fn player_entry(frame: &DriverEntriesFrame) -> &DriverEntry {
    frame
        .entries
        .iter()
        .find(|entry| entry.is_player)
        .expect("player entry")
}

#[test]
fn test_unpublished_live_positions_rank_the_player_by_results() {
    let session = unpublished_positions_session(SessionType::Race);
    let frame = compute_unpublished(
        &unpublished_positions_frame(),
        &session,
        SessionState::Racing,
    );

    let player = player_entry(&frame);

    assert_eq!(player.position, 3);
    assert_eq!(player.class_position, 3);

    let mut official: Vec<&DriverEntry> = frame.entries.iter().collect();
    official.sort_by_key(|entry| official_sort_key(entry));

    assert_eq!(official.last().map(|entry| entry.car_idx), Some(0));
}

#[test]
fn test_unpublished_live_positions_place_the_player_after_racing_cars() {
    let session = unpublished_positions_session(SessionType::Race);
    let frame = compute_unpublished(
        &unpublished_positions_frame(),
        &session,
        SessionState::Racing,
    );

    let order: Vec<i32> = frame.entries.iter().map(|entry| entry.car_idx).collect();

    assert_eq!(order, vec![1, 2, 0]);
    assert_eq!(player_entry(&frame).live_position, 3);
}

#[test]
fn test_unpublished_live_positions_do_not_project_the_player_as_winner() {
    let session = unpublished_positions_session(SessionType::Race);
    let frame = compute_unpublished(
        &unpublished_positions_frame(),
        &session,
        SessionState::Racing,
    );

    let player_delta = player_entry(&frame).estimated_ir_delta_official;
    let leader_delta = frame
        .entries
        .iter()
        .find(|entry| entry.car_idx == 1)
        .and_then(|entry| entry.estimated_ir_delta_official);

    assert!(player_delta.is_some_and(|delta| delta < 0));
    assert!(leader_delta.is_some_and(|delta| delta > 0));
}

#[test]
fn test_only_official_races_project_irating() {
    let mut unofficial_race = unpublished_positions_session(SessionType::Race);
    unofficial_race.is_official = false;

    let mut league_race = unpublished_positions_session(SessionType::Race);
    league_race.league_id = Some(1);

    for session in [
        unpublished_positions_session(SessionType::Practice),
        unpublished_positions_session(SessionType::Qualify),
        unofficial_race,
        league_race,
    ] {
        let frame = compute_unpublished(
            &unpublished_positions_frame(),
            &session,
            SessionState::Racing,
        );

        assert!(frame.entries.iter().all(|entry| {
            entry.estimated_ir_delta_live.is_none() && entry.estimated_ir_delta_official.is_none()
        }));
    }
}

#[test]
fn test_published_live_positions_still_win_over_results() {
    let session = unpublished_positions_session(SessionType::Race);
    let mut car_idx = unpublished_positions_frame();
    car_idx.car_idx_position = vec![1, 3, 2];
    car_idx.car_idx_class_position = vec![1, 3, 2];

    let frame = compute_unpublished(&car_idx, &session, SessionState::Racing);

    assert_eq!(player_entry(&frame).position, 1);
    assert_eq!(player_entry(&frame).class_position, 1);
}

#[test]
fn test_zeroed_positions_before_the_green_keep_the_grid_order() {
    let mut session = unpublished_positions_session(SessionType::Race);
    session.sessions[0].results_positions.clear();

    let mut car_idx = unpublished_positions_frame();
    car_idx.car_idx_position = vec![0, 0, 0];
    car_idx.car_idx_class_position = vec![0, 0, 0];

    let start_positions: HashMap<i32, (i32, i32)> = [(0, (2, 2)), (1, (3, 3)), (2, (1, 1))]
        .into_iter()
        .collect();
    let state = Mutex::new(DriverEntriesState::default());
    let frame = compute(
        &car_idx,
        &session,
        &start_positions,
        false,
        Some(SessionState::ParadeLaps),
        &state,
    );

    let order: Vec<i32> = frame.entries.iter().map(|entry| entry.car_idx).collect();

    assert_eq!(order, vec![2, 0, 1]);
}

#[test]
fn test_grid_position_does_not_change_when_a_car_ahead_loads() {
    let session = two_car_race_session();
    let start_positions = HashMap::from([(0, (2, 2)), (1, (1, 1))]);
    let state = Mutex::new(DriverEntriesState::default());
    let mut telemetry = two_car_frame(TrackSurface::NotInWorld, -1.0, 0.1);
    telemetry.car_idx_position = vec![0, 0];
    telemetry.car_idx_class_position = vec![0, 0];

    let before = compute(
        &telemetry,
        &session,
        &start_positions,
        false,
        Some(SessionState::ParadeLaps),
        &state,
    );
    assert_eq!(player_entry(&before).live_position, 2);
    assert_eq!(player_entry(&before).live_class_position, 2);
    assert!(before.entries.iter().any(|entry| entry.car_idx == 1));

    telemetry.car_idx_track_surface[1] = TrackSurface::OnTrack;
    telemetry.car_idx_lap_dist_pct[1] = 0.2;
    let after = compute(
        &telemetry,
        &session,
        &start_positions,
        false,
        Some(SessionState::ParadeLaps),
        &state,
    );
    assert_eq!(player_entry(&after).live_position, 2);
    assert_eq!(player_entry(&after).live_class_position, 2);
}

#[test]
fn test_official_grid_puts_unqualified_cars_in_irating_order() {
    let mut session = unpublished_positions_session(SessionType::Race);
    session.sessions[0].results_positions.clear();
    session.cars[0].i_rating = 1500;
    session.cars[1].i_rating = 2500;
    session.cars[2].i_rating = 1000;
    session.qualify_results = vec![QualifyResultEntry {
        car_idx: 2,
        position: 1,
        fastest_time: Some(90.0),
        ..Default::default()
    }];
    let mut telemetry = unpublished_positions_frame();
    telemetry.car_idx_position = vec![0, 0, 0];
    telemetry.car_idx_class_position = vec![0, 0, 0];
    telemetry.car_idx_lap_dist_pct = vec![-1.0, -1.0, -1.0];
    telemetry.car_idx_track_surface = vec![TrackSurface::NotInWorld; 3];
    let grid = compute(
        &telemetry,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::ParadeLaps),
        &Mutex::new(DriverEntriesState::default()),
    );
    assert_eq!(
        grid.entries
            .iter()
            .map(|entry| entry.car_idx)
            .collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
    assert_eq!(player_entry(&grid).position, 3);
    assert_eq!(player_entry(&grid).live_class_position, 3);
}

#[test]
fn test_official_grid_without_qualifying_uses_irating() {
    let mut session = two_car_race_session();
    session.is_official = true;
    session.cars[0].i_rating = 1500;
    session.cars[1].i_rating = 2500;
    let mut telemetry = two_car_frame(TrackSurface::NotInWorld, -1.0, -1.0);
    telemetry.car_idx_position = vec![0, 0];
    let grid = compute(
        &telemetry,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::ParadeLaps),
        &Mutex::new(DriverEntriesState::default()),
    );
    assert_eq!(
        grid.entries
            .iter()
            .map(|entry| entry.car_idx)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
}

#[test]
fn test_multiclass_grid_groups_classes_before_qualifying_and_irating() {
    let mut session = unpublished_positions_session(SessionType::Race);
    session.sessions[0].results_positions.clear();
    session.cars[0].car_class_id = 1;
    session.cars[0].car_class_est_lap_time = 90.0;
    session.cars[0].i_rating = 1000;
    session.cars[1].car_class_id = 2;
    session.cars[1].car_class_est_lap_time = 110.0;
    session.cars[1].i_rating = 2500;
    session.cars[2].car_class_id = 2;
    session.cars[2].car_class_est_lap_time = 110.0;
    session.cars[2].i_rating = 1000;
    session.qualify_results = vec![QualifyResultEntry {
        car_idx: 2,
        position: 1,
        fastest_time: Some(100.0),
        ..Default::default()
    }];
    let mut telemetry = unpublished_positions_frame();
    telemetry.car_idx_position = vec![0, 0, 0];
    telemetry.car_idx_lap_dist_pct = vec![-1.0; 3];
    telemetry.car_idx_track_surface = vec![TrackSurface::NotInWorld; 3];
    let grid = compute(
        &telemetry,
        &session,
        &HashMap::new(),
        false,
        Some(SessionState::GetInCar),
        &Mutex::new(DriverEntriesState::default()),
    );
    assert_eq!(
        grid.entries
            .iter()
            .map(|entry| entry.car_idx)
            .collect::<Vec<_>>(),
        vec![0, 2, 1]
    );
    assert_eq!(
        grid.entries
            .iter()
            .map(|entry| entry.live_class_position)
            .collect::<Vec<_>>(),
        vec![1, 1, 2]
    );
}

#[test]
fn test_official_grid_uses_sim_results_when_available() {
    let mut session = two_car_race_session();
    session.is_official = true;
    session.cars[0].i_rating = 2500;
    session.cars[1].i_rating = 1000;
    session.sessions[0].results_positions = vec![result_at(1, 1), result_at(0, 2)];
    let telemetry = two_car_frame(TrackSurface::NotInWorld, -1.0, -1.0);
    let positions = HashMap::from([(1, (1, 1)), (0, (2, 2))]);
    let grid = compute(
        &telemetry,
        &session,
        &positions,
        false,
        Some(SessionState::ParadeLaps),
        &Mutex::new(DriverEntriesState::default()),
    );
    assert_eq!(
        grid.entries
            .iter()
            .map(|entry| entry.car_idx)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
}

#[test]
fn test_player_in_the_garage_in_qualifying_keeps_the_place_set_by_time() {
    let session = unpublished_positions_session(SessionType::Qualify);
    let frame = compute_unpublished(
        &unpublished_positions_frame(),
        &session,
        SessionState::Racing,
    );

    assert_eq!(player_entry(&frame).position, 3);
    assert_eq!(player_entry(&frame).live_position, 3);
}

#[test]
fn test_pit_state_reset_clears_all_states() {
    let mut processor = DriverEntriesProcessor::default();
    processor.reset();
    let locked = processor.state.lock().unwrap();
    assert!(locked.pit_states.is_empty());
}
