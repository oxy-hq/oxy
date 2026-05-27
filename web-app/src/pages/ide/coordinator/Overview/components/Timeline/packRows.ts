import type { NormalizedRun } from "../../../components/runModel";

export interface PlacedRun {
  run: NormalizedRun;
  /** Sub-row within the lane, so overlapping runs never collide. */
  row: number;
  startMs: number;
  endMs: number;
}

/**
 * Greedy interval packing — assign each run the first sub-row whose previous
 * run has already finished. Keeps a swimlane compact without overlap.
 */
export const packRows = (
  runs: NormalizedRun[],
  nowMs: number
): {
  placed: PlacedRun[];
  rowCount: number;
} => {
  const rowEnds: number[] = [];
  const placed: PlacedRun[] = [];
  const sorted = [...runs].sort(
    (a, b) => new Date(a.startedAt).getTime() - new Date(b.startedAt).getTime()
  );
  for (const run of sorted) {
    const startMs = new Date(run.startedAt).getTime();
    const endMs = run.endedAt ? new Date(run.endedAt).getTime() : nowMs;
    let row = rowEnds.findIndex((end) => end <= startMs);
    if (row === -1) {
      row = rowEnds.length;
      rowEnds.push(endMs);
    } else {
      rowEnds[row] = endMs;
    }
    placed.push({ run, row, startMs, endMs });
  }
  return { placed, rowCount: Math.max(1, rowEnds.length) };
};
