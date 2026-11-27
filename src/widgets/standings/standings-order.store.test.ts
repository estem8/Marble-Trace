import { describe, expect, it, vi } from 'vitest';
import type { CarIdentity } from '@shared/contracts/car-identity';
import { MOVE_DURATION_MS } from '@shared/lib/animation';
import { StandingsOrderStore } from './standings-order.store';

const ARROW_LINGER_MS = 400;

const entry = (carIdx: number, overall: number, inClass = overall) =>
  ({
    carIdx,
    carClassId: 1,
    livePosition: overall,
    liveClassPosition: inClass,
  }) as CarIdentity;

describe('StandingsOrderStore', () => {
  it('clears position arrows when rebuilding the table', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1), entry(2, 2)], true, 0);
      order.process([entry(1, 2), entry(2, 1)], true, 100);
      expect(order.positionChanges.get(1)).toBe('down');

      order.reset();
      expect(order.positionChanges.size).toBe(0);
    } finally {
      order.dispose();
    }
  });

  it('clears the arrow when a car leaves and starts fresh when it returns', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1), entry(2, 2)], true, 0);
      order.process([entry(1, 2), entry(2, 1)], true, 100);
      expect(order.positionChanges.get(1)).toBe('down');

      order.process([entry(2, 1)], true, 200);
      expect(order.positionChanges.has(1)).toBe(false);

      order.process([entry(1, 3), entry(2, 1)], true, 300);
      expect(order.positionChanges.has(1)).toBe(false);
    } finally {
      order.dispose();
    }
  });

  it('does not move a retained class position into an occupied class', () => {
    const order = new StandingsOrderStore();
    const first = { ...entry(1, 1, 2), carClassId: 1 };
    const second = { ...entry(2, 1, 1), carClassId: 2 };
    const transferring = { ...entry(1, 2, 1), carClassId: 2 };

    try {
      order.process([first, second], true, 0);
      order.process([transferring, second], true, 100);
      order.process(null, true, 1100);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 2, inClass: 2 });
      expect(order.renderedRanks.get(2)?.inClass).toBe(1);

      order.process(
        [transferring, { ...second, liveClassPosition: 2 }],
        true,
        1200
      );
      order.process(null, true, 2200);
      expect(order.renderedRanks.get(1)?.inClass).toBe(1);
      expect(order.renderedRanks.get(2)?.inClass).toBe(2);
    } finally {
      order.dispose();
    }
  });

  it.each([
    { ranks: [4, 4], classRanks: [4, 5] },
    { ranks: [4, 5], classRanks: [4, 4] },
  ])(
    'holds moves with conflicting destinations ($ranks / $classRanks)',
    ({ ranks, classRanks }) => {
      const order = new StandingsOrderStore();

      try {
        order.process([entry(1, 1), entry(2, 2)], true, 0);
        order.process(
          [
            entry(1, ranks[0], classRanks[0]),
            entry(2, ranks[1], classRanks[1]),
          ],
          true,
          100
        );
        order.process(null, true, 1000);

        expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 1 });
        expect(order.renderedRanks.get(2)).toEqual({ overall: 2, inClass: 2 });
        expect(order.hasPending()).toBe(true);

        order.process(
          [entry(1, ranks[0], classRanks[0]), entry(2, 5)],
          true,
          1100
        );

        expect(order.renderedRanks.get(1)).toEqual({ overall: 4, inClass: 4 });
        expect(order.renderedRanks.get(2)).toEqual({ overall: 2, inClass: 2 });
        order.process(null, true, 2000);
        expect(order.renderedRanks.get(2)).toEqual({ overall: 5, inClass: 5 });
      } finally {
        order.dispose();
      }
    }
  );

  it('leaves contested initial positions blank until the ranks resolve', () => {
    const order = new StandingsOrderStore();

    try {
      const holding = entry(1, 1);
      order.process([holding, entry(2, 1), entry(3, 3)], true, 0);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 0, inClass: 0 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 0 });

      order.process([holding, entry(2, 2), entry(3, 3)], true, 100);

      expect(order.renderedRanks.get(1)?.overall).toBe(1);
      expect(order.renderedRanks.get(2)?.overall).toBe(2);
      expect(order.renderedRanks.get(3)?.overall).toBe(3);
    } finally {
      order.dispose();
    }
  });

  it('settles unique class positions while overall positions remain contested', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1), entry(2, 1)], true, 0);
      order.process([entry(1, 1, 1), entry(2, 1, 2)], true, 100);

      expect(order.renderedRanks.get(1)).toEqual({ overall: 0, inClass: 1 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 2 });
    } finally {
      order.dispose();
    }
  });

  it('does not settle a class move whose dependency is blocked', () => {
    const order = new StandingsOrderStore();

    try {
      order.process(
        [entry(1, 1, 1), entry(2, 1, 2), entry(3, 1, 3), entry(4, 1, 4)],
        true,
        0
      );
      order.process(
        [entry(1, 1, 2), entry(2, 1, 3), entry(3, 1, 4), entry(4, 1, 4)],
        true,
        100
      );

      expect(order.renderedRanks.get(1)).toEqual({ overall: 0, inClass: 1 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 2 });
    } finally {
      order.dispose();
    }
  });

  it('waits for the settle delay when an already-ranked car gets a class rank', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 0), entry(2, 2, 2)], true, 0);
      order.process([entry(1, 2, 1), entry(2, 1, 2)], true, 100);

      expect(order.renderedRanks.get(1)?.overall).toBe(1);
      expect(order.renderedRanks.get(2)?.overall).toBe(2);

      order.process(null, true, 999);
      expect(order.renderedRanks.get(1)?.overall).toBe(1);
      order.process(null, true, 1000);
      expect(order.renderedRanks.get(1)?.overall).toBe(2);
      expect(order.renderedRanks.get(2)?.overall).toBe(1);
    } finally {
      order.dispose();
    }
  });

  it('settles class ranks around an occupied overall rank independently', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 2, 2), entry(3, 2, 3)], true, 0);
      order.process(
        [entry(1, 1, 1), entry(2, 1, 3), entry(3, 3, 2)],
        true,
        100
      );

      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 3 });
      expect(order.renderedRanks.get(3)).toEqual({ overall: 3, inClass: 2 });
    } finally {
      order.dispose();
    }
  });

  it('settles unique overall positions while class positions remain contested', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 1, 1)], true, 0);
      order.process([entry(1, 1, 1), entry(2, 2, 1)], true, 100);

      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 0 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 2, inClass: 0 });
    } finally {
      order.dispose();
    }
  });

  it('clears the arrow after an overall move with a contested class rank', () => {
    vi.useFakeTimers();
    const order = new StandingsOrderStore();
    const first = entry(1, 1, 1);
    const second = entry(2, 1, 1);
    const blocker = entry(3, 2, 1);
    const moved = entry(1, 2, 1);

    try {
      order.process([first, second, blocker], true, 0);
      order.process([moved, second, blocker], true, 100);
      expect(order.renderedRanks.get(1)?.overall).toBe(0);
      expect(order.positionChanges.get(1)).toBe('down');

      order.process([moved, second], true, 200);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 2, inClass: 0 });
      expect(order.positionChanges.get(1)).toBe('down');

      vi.advanceTimersByTime(MOVE_DURATION_MS + ARROW_LINGER_MS);
      expect(order.positionChanges.has(1)).toBe(false);
    } finally {
      order.dispose();
      vi.useRealTimers();
    }
  });

  it('settles unset class ranks after overall ranks already settle', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 1, 1)], true, 0);
      order.process([entry(1, 1, 1), entry(2, 2, 1)], true, 100);
      order.process([entry(1, 1, 1), entry(2, 1, 2)], true, 200);

      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 1 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 2, inClass: 2 });
    } finally {
      order.dispose();
    }
  });

  it('uses the short arrow timeout for an overall move placed immediately', () => {
    vi.useFakeTimers();
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 1, 1)], true, 0);
      order.process([entry(1, 2, 1), entry(2, 1, 1)], true, 100);
      expect(order.renderedRanks.get(1)?.overall).toBe(2);
      expect(order.positionChanges.get(1)).toBe('down');

      vi.advanceTimersByTime(MOVE_DURATION_MS + ARROW_LINGER_MS);
      expect(order.positionChanges.has(1)).toBe(false);
    } finally {
      order.dispose();
      vi.useRealTimers();
    }
  });

  it('waits before releasing a class rank with overall ranks contested', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 1, 2)], true, 0);
      order.process([entry(1, 1, 0), entry(2, 1, 1)], true, 100);

      order.process(null, true, 999);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 0, inClass: 1 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 2 });

      order.process(null, true, 1000);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 0, inClass: 0 });
      expect(order.renderedRanks.get(2)).toEqual({ overall: 0, inClass: 1 });
    } finally {
      order.dispose();
    }
  });

  it('keeps a class rank through a brief dropout during first overall placement', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 1, 2)], true, 0);
      order.process([entry(1, 1, 0), entry(2, 2, 2)], true, 100);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 1 });

      order.process([entry(1, 1, 1), entry(2, 2, 2)], true, 200);
      order.process(null, true, 1000);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 1 });
    } finally {
      order.dispose();
    }
  });

  it('clears a missing class rank even while the overall rank changes', () => {
    const order = new StandingsOrderStore();

    try {
      order.process([entry(1, 1, 1), entry(2, 2, 2)], true, 0);
      order.process([entry(1, 1, 0), entry(2, 2, 2)], true, 100);
      order.process([entry(1, 3, 0), entry(2, 2, 2)], true, 600);

      order.process(null, true, 999);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 1 });
      order.process(null, true, 1000);
      expect(order.renderedRanks.get(1)).toEqual({ overall: 1, inClass: 0 });
    } finally {
      order.dispose();
    }
  });

  it('places a newly ranked car without changing the timer of an unrelated swap', () => {
    const order = new StandingsOrderStore();

    try {
      order.process(
        [entry(1, 1), entry(2, 0), entry(3, 3), entry(4, 4)],
        true,
        0
      );
      order.process(
        [entry(1, 1), entry(2, 0), entry(3, 4), entry(4, 3)],
        true,
        100
      );
      order.process(
        [entry(1, 2), entry(2, 1), entry(3, 4), entry(4, 3)],
        true,
        200
      );

      expect(
        [...order.renderedRanks.values()].map((rank) => rank.overall)
      ).toEqual([2, 1, 3, 4]);
      order.process(null, true, 999);
      expect(order.renderedRanks.get(3)?.overall).toBe(3);
      order.process(null, true, 1000);
      expect(order.renderedRanks.get(3)?.overall).toBe(4);
      expect(order.renderedRanks.get(4)?.overall).toBe(3);
    } finally {
      order.dispose();
    }
  });
});
