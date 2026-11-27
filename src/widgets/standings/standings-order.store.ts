import { makeAutoObservable, runInAction } from 'mobx';
import type { CarIdentity } from '@shared/contracts/car-identity';
import { MOVE_DURATION_MS } from '@shared/lib/animation';

const ORDER_SETTLE_MS = 900;
const POSITION_CHANGE_MAX_MS = 3000;
const ARROW_LINGER_MS = 400;

export type Rank = { overall: number; inClass: number };
type PositionChangeDirection = 'up' | 'down';
type PendingPosition = { rank: Rank; classId: number; since: number };

const sameRank = (first: Rank, second: Rank): boolean =>
  first.overall === second.overall && first.inClass === second.inClass;

const rankKeys = (rank: Rank, classId: number): string[] => {
  const keys: string[] = [];

  if (rank.overall > 0) {
    keys.push(String(rank.overall));
  }

  if (rank.inClass > 0) {
    keys.push(`${classId}:${rank.inClass}`);
  }

  return keys;
};

const readRank = (entry: CarIdentity, useTrackOrder: boolean): Rank =>
  useTrackOrder
    ? { overall: entry.livePosition, inClass: entry.liveClassPosition }
    : { overall: entry.position, inClass: entry.classPosition };

// A rank of zero or below is a car the sim has not placed yet (no lap in
// qualifying). It sorts after every placed car instead of above the leader.
const isPlaced = (rank: number): boolean => rank > 0;

export const compareRanks = (first: number, second: number): number => {
  if (isPlaced(first) && isPlaced(second)) {
    return first - second;
  }

  if (isPlaced(first)) {
    return -1;
  }

  if (isPlaced(second)) {
    return 1;
  }

  return 0;
};

/** Debounced ranks and position arrows for one standings table. */
export class StandingsOrderStore {
  readonly positionChanges = new Map<number, PositionChangeDirection>();
  private readonly settled = new Map<number, Rank>();
  private readonly pending = new Map<number, PendingPosition>();
  private readonly classIds = new Map<number, number>();
  private readonly occupants = new Map<string, Set<number>>();
  private readonly previousPositions = new Map<number, number>();
  private readonly changeTimers = new Map<
    number,
    ReturnType<typeof setTimeout>
  >();

  constructor() {
    makeAutoObservable<
      StandingsOrderStore,
      | 'pending'
      | 'classIds'
      | 'occupants'
      | 'previousPositions'
      | 'changeTimers'
    >(this, {
      pending: false,
      classIds: false,
      occupants: false,
      previousPositions: false,
      changeTimers: false,
    });
  }

  get renderedRanks(): ReadonlyMap<number, Rank> {
    return this.settled;
  }

  hasPending(): boolean {
    return this.pending.size > 0;
  }

  /** A motion-only frame can finish a pass without scanning the field again. */
  process(entries: CarIdentity[] | null, useTrackOrder: boolean, now: number) {
    if (entries) {
      this.updateOrder(entries, useTrackOrder, now);
    }

    this.settlePending(now);

    if (entries) {
      this.trackPositionChanges(entries, useTrackOrder);
    }
  }

  reset() {
    this.clearRanks();
    this.previousPositions.clear();

    for (const timer of this.changeTimers.values()) {
      clearTimeout(timer);
    }

    this.changeTimers.clear();
    this.positionChanges.clear();
  }

  dispose() {
    this.reset();
  }

  private clearRanks() {
    this.settled.clear();
    this.pending.clear();
    this.classIds.clear();
    this.occupants.clear();
  }

  private updateOrder(
    entries: CarIdentity[],
    useTrackOrder: boolean,
    now: number
  ) {
    const next = new Map(
      entries.map((entry) => [
        entry.carIdx,
        { rank: readRank(entry, useTrackOrder), classId: entry.carClassId },
      ])
    );

    for (const [carIdx, rank] of this.settled) {
      if (!next.has(carIdx)) {
        this.releaseRanks(carIdx, rank);
        this.settled.delete(carIdx);
        this.classIds.delete(carIdx);
        this.pending.delete(carIdx);
        this.previousPositions.delete(carIdx);
        clearTimeout(this.changeTimers.get(carIdx));
        this.changeTimers.delete(carIdx);
        this.positionChanges.delete(carIdx);
      }
    }

    if (this.settled.size === 0) {
      const claims = new Map<string, number>();

      for (const { rank, classId } of next.values()) {
        for (const key of rankKeys(rank, classId)) {
          claims.set(key, (claims.get(key) ?? 0) + 1);
        }
      }

      for (const [carIdx, { rank, classId }] of next) {
        this.setRank(
          carIdx,
          {
            overall:
              (claims.get(String(rank.overall)) ?? 0) > 1 ? 0 : rank.overall,
            inClass:
              (claims.get(`${classId}:${rank.inClass}`) ?? 0) > 1
                ? 0
                : rank.inClass,
          },
          classId
        );
      }

      return;
    }

    for (const [carIdx, { rank, classId }] of next) {
      const settled = this.settled.get(carIdx);

      if (!settled) {
        this.setRank(carIdx, { overall: 0, inClass: 0 }, classId);
      } else if (
        sameRank(settled, rank) &&
        this.classIds.get(carIdx) === classId
      ) {
        this.pending.delete(carIdx);
        continue;
      }

      const pending = this.pending.get(carIdx);

      if (
        !pending ||
        pending.classId !== classId ||
        !sameRank(pending.rank, rank)
      ) {
        this.pending.set(carIdx, { rank, classId, since: now });
      }
    }
  }

  private settlePending(now: number) {
    const ready = new Map<number, PendingPosition>();
    const claims = new Map<string, number>();
    const immediate = new Set<number>();
    const deferredClassClears = new Set<number>();

    for (const [carIdx, pending] of this.pending) {
      const { rank, classId } = pending;
      const previousRank = this.settled.get(carIdx);

      for (const key of rankKeys(rank, classId)) {
        claims.set(key, (claims.get(key) ?? 0) + 1);
      }

      if (now - pending.since >= ORDER_SETTLE_MS) {
        ready.set(carIdx, pending);
      } else if (
        isPlaced(previousRank?.inClass ?? 0) &&
        !isPlaced(rank.inClass)
      ) {
        deferredClassClears.add(carIdx);
      }

      if (
        !isPlaced(previousRank?.overall ?? 0) ||
        this.classIds.get(carIdx) !== classId
      ) {
        immediate.add(carIdx);
      }
    }

    const uniqueClassMoves = new Set<number>();

    for (const [carIdx, { rank, classId }] of this.pending) {
      const settled = this.settled.get(carIdx);
      const classKey = `${classId}:${rank.inClass}`;

      if (
        settled &&
        this.classIds.get(carIdx) === classId &&
        !deferredClassClears.has(carIdx) &&
        (settled.overall === 0 ||
          !isPlaced(settled.inClass) ||
          !isPlaced(rank.inClass)) &&
        settled.inClass !== rank.inClass &&
        (!isPlaced(rank.inClass) || claims.get(classKey) === 1)
      ) {
        uniqueClassMoves.add(carIdx);
      }
    }

    let classMoveRemoved = true;

    while (classMoveRemoved) {
      classMoveRemoved = false;

      for (const carIdx of uniqueClassMoves) {
        const pending = this.pending.get(carIdx);

        if (!pending) {
          uniqueClassMoves.delete(carIdx);
          classMoveRemoved = true;
          continue;
        }

        const classKey = `${pending.classId}:${pending.rank.inClass}`;
        const blocked =
          pending.rank.inClass > 0 &&
          [...(this.occupants.get(classKey) ?? [])].some(
            (occupant) => occupant !== carIdx && !uniqueClassMoves.has(occupant)
          );

        if (blocked) {
          uniqueClassMoves.delete(carIdx);
          classMoveRemoved = true;
        }
      }
    }

    const uniqueOverallMoves = new Set<number>();

    for (const [carIdx, { rank }] of this.pending) {
      const settled = this.settled.get(carIdx);
      const overallKey = String(rank.overall);

      if (
        settled &&
        settled.overall === 0 &&
        rank.overall > 0 &&
        claims.get(overallKey) === 1 &&
        !this.occupants.has(overallKey)
      ) {
        uniqueOverallMoves.add(carIdx);
      }
    }

    const partialMoves = new Set([...uniqueClassMoves, ...uniqueOverallMoves]);

    for (const carIdx of partialMoves) {
      const settled = this.settled.get(carIdx);

      if (settled) {
        this.releaseRanks(carIdx, settled);
      }
    }

    for (const carIdx of partialMoves) {
      const pending = this.pending.get(carIdx);
      const settled = this.settled.get(carIdx);

      if (pending && settled) {
        const movesOverall = uniqueOverallMoves.has(carIdx);
        this.setRank(
          carIdx,
          {
            overall: movesOverall ? pending.rank.overall : settled.overall,
            inClass: uniqueClassMoves.has(carIdx)
              ? pending.rank.inClass
              : settled.inClass,
          },
          this.classIds.get(carIdx) ?? pending.classId
        );

        if (movesOverall) {
          this.clearArrowAfterMove(carIdx);
        }
      }
    }

    for (const carIdx of immediate) {
      for (const [index, move] of this.immediateMoves(
        carIdx,
        claims,
        deferredClassClears
      ) ?? []) {
        ready.set(index, move);
      }
    }

    // Land only unique claims whose occupants leave in this batch. Removing
    // a move can block another, so repeat until the remaining moves can land.
    let removed = true;

    while (removed) {
      removed = false;

      for (const [carIdx, { rank, classId }] of ready) {
        const blocked = rankKeys(rank, classId).some((key) => {
          if ((claims.get(key) ?? 0) > 1) {
            return true;
          }

          for (const occupant of this.occupants.get(key) ?? []) {
            if (occupant !== carIdx && !ready.has(occupant)) {
              return true;
            }
          }

          return false;
        });

        if (blocked) {
          ready.delete(carIdx);
          removed = true;
        }
      }
    }

    for (const carIdx of ready.keys()) {
      const rank = this.settled.get(carIdx);

      if (rank) {
        this.releaseRanks(carIdx, rank);
      }
    }

    for (const [carIdx, { rank, classId }] of ready) {
      this.setRank(carIdx, rank, classId);
      this.pending.delete(carIdx);
      this.clearArrowAfterMove(carIdx);
    }
  }

  // A first placement brings along its dependencies only if they can all move.
  private immediateMoves(
    carIdx: number,
    claims: ReadonlyMap<string, number>,
    deferredClassClears: ReadonlySet<number>
  ) {
    const required = new Set([carIdx]);
    const moves = new Map<number, PendingPosition>();

    for (const index of required) {
      const pending = this.pending.get(index);

      if (!pending || deferredClassClears.has(index)) {
        return null;
      }

      for (const key of rankKeys(pending.rank, pending.classId)) {
        if ((claims.get(key) ?? 0) > 1) {
          return null;
        }

        for (const occupant of this.occupants.get(key) ?? []) {
          required.add(occupant);
        }
      }

      moves.set(index, pending);
    }

    return moves;
  }

  private setRank(carIdx: number, rank: Rank, classId: number) {
    this.settled.set(carIdx, rank);
    this.classIds.set(carIdx, classId);

    for (const key of rankKeys(rank, classId)) {
      const occupants = this.occupants.get(key) ?? new Set<number>();

      occupants.add(carIdx);
      this.occupants.set(key, occupants);
    }
  }

  private releaseRanks(carIdx: number, rank: Rank) {
    const classId = this.classIds.get(carIdx);

    if (classId === undefined) {
      return;
    }

    for (const key of rankKeys(rank, classId)) {
      const occupants = this.occupants.get(key);
      occupants?.delete(carIdx);

      if (occupants?.size === 0) {
        this.occupants.delete(key);
      }
    }
  }

  private trackPositionChanges(entries: CarIdentity[], useTrackOrder: boolean) {
    for (const entry of entries) {
      const previous = this.previousPositions.get(entry.carIdx);
      const current = useTrackOrder ? entry.livePosition : entry.position;

      this.previousPositions.set(entry.carIdx, current);

      if (
        previous === undefined ||
        previous === current ||
        !isPlaced(previous) ||
        !isPlaced(current)
      ) {
        continue;
      }

      this.flashPositionChange(
        entry.carIdx,
        current < previous ? 'up' : 'down'
      );
    }
  }

  private flashPositionChange(
    carIdx: number,
    direction: PositionChangeDirection
  ) {
    this.positionChanges.set(carIdx, direction);
    this.clearChangeAfter(carIdx, POSITION_CHANGE_MAX_MS);
  }

  // The row starts sliding on the render that follows the settle, so the arrow
  // is given the move duration plus a short tail before it hands the cell back
  // to the position number.
  private clearArrowAfterMove(carIdx: number) {
    if (!this.positionChanges.has(carIdx)) {
      return;
    }

    this.clearChangeAfter(carIdx, MOVE_DURATION_MS + ARROW_LINGER_MS);
  }

  private clearChangeAfter(carIdx: number, delayMs: number) {
    const running = this.changeTimers.get(carIdx);

    if (running) {
      clearTimeout(running);
    }

    this.changeTimers.set(
      carIdx,
      setTimeout(() => {
        runInAction(() => {
          this.positionChanges.delete(carIdx);
          this.changeTimers.delete(carIdx);
        });
      }, delayMs)
    );
  }
}
