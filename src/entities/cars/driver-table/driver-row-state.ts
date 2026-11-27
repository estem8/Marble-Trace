import type { CarIdentity } from '@shared/contracts/car-identity';
import type { FlagType } from '@shared/contracts/domain';
import {
  parseDriverFlags,
  TRACK_SURFACE_IN_PIT_STALL,
  TRACK_SURFACE_OFF_TRACK,
} from '@shared/lib/driver';

/** What a driver row shows about the car beyond its numbers. */
export interface DriverRowState {
  isOut: boolean;
  isPit: boolean;
  isTowed: boolean;
  isOffTrack: boolean;
  isFinished: boolean;
  raceOutcome?: NonNullable<CarIdentity['raceOutcome']>;
  flagType: FlagType;
}

export interface DriverRowContext {
  /** The session is over: everybody drops to the garage, which is no longer a status. */
  isSessionEnded: boolean;
  /** The race grid is still forming; an absent car may be waiting to grid. */
  isGridding: boolean;
  /** The player's tow is counting down — the car sits in the stall, towed, not stopped. */
  isPlayerTowing: boolean;
  /** A race's checkered badge comes from the per-car finish observation. */
  requireConfirmedFinish?: boolean;
}

/**
 * The statuses of one table row — Standings and Relative read them the same
 * way.
 *
 * Finishing is latched in the backend once the car crosses the line under the
 * checkered flag, so it outlives the drive back to the garage. Official DNF
 * and DQ take precedence. A towed car left the world without
 * going through the pit lane, which OUT alone would not tell apart from a
 * garage exit.
 */
export const driverRowStateOf = (
  driver: CarIdentity,
  context: DriverRowContext
): DriverRowState => {
  const raceOutcome = driver.raceOutcome ?? undefined;
  const isFinished = driver.isFinished && !raceOutcome;
  const rawFlag = parseDriverFlags(driver.rawFlags);
  const isInGarage = driver.trackSurface === 'NotInWorld';
  const isOut =
    !isFinished &&
    (driver.isRetired ||
      (isInGarage && !context.isSessionEnded && !context.isGridding));

  return {
    isFinished,
    raceOutcome,
    isOut,
    isPit:
      !isFinished &&
      !isOut &&
      (driver.trackSurface === TRACK_SURFACE_IN_PIT_STALL || driver.onPitRoad),
    isTowed:
      (driver.isTowed || (driver.isPlayer && context.isPlayerTowing)) &&
      !isFinished,
    isOffTrack: !isOut && driver.trackSurface === TRACK_SURFACE_OFF_TRACK,
    flagType:
      raceOutcome === 'dq'
        ? 'dq'
        : raceOutcome === 'dnf' ||
            (rawFlag === 'checkered' && context.requireConfirmedFinish)
          ? 'none'
          : rawFlag,
  };
};
