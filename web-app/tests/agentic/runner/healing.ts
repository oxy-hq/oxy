// Tier 2 healing — when every recorded selector strategy for one action
// fails on replay, the runtime does an intent-aware redrive (same cost
// as a cold step, but starting from the partially-replayed page state)
// and stages the new recording to `.cache/healing-staging.json`. A
// developer must explicitly accept the staged recording via
// `pnpm test:agentic --accept-healing <flow>` to promote it into
// `.cache/bespoke-actions.json`.
//
// Why a staging file (vs auto-promote): a complete strategy failure
// means the UI was *fundamentally* redesigned, not just a label tweak.
// That deserves a code-review event before the new selectors become
// the new ground truth.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { CACHE_VERSION, type CacheFile, type RecordedAction } from "./action-cache";

interface HealingStagingEntry {
  flow: string;
  case: string;
  step_index: number;
  /** sha256 cache key, same as the main cache. */
  cache_key: string;
  staged_at: string;
  actions: RecordedAction[];
}

interface HealingStagingFile {
  entries: HealingStagingEntry[];
}

interface HealingArtifactEntry {
  flow: string;
  case: string;
  step_index: number;
  action_index: number;
  drift: {
    old_primary?: string;
    new_primary?: string;
    old_kind?: string;
    new_kind?: string;
    intent?: string;
  };
}

/** Append the new actions to the staging file. */
export function stageHealedActions(
  stagingPath: string,
  entry: Omit<HealingStagingEntry, "staged_at">
): void {
  const file = loadStaging(stagingPath);
  file.entries.push({ ...entry, staged_at: new Date().toISOString() });
  persist(stagingPath, file);
}

/** Read all staged entries that match a flow filter (by name). */
export function readStaging(stagingPath: string): HealingStagingFile {
  return loadStaging(stagingPath);
}

/**
 * Promote staged recordings for a flow into the main action-cache file.
 * Returns the count promoted. Callers usually print a `git status` hint
 * after so the developer sees the diff.
 */
export function promoteStaging(
  stagingPath: string,
  cachePath: string,
  flowFilter?: string
): number {
  const staging = loadStaging(stagingPath);
  if (staging.entries.length === 0) return 0;
  const promote = flowFilter
    ? staging.entries.filter((e) => e.flow === flowFilter)
    : staging.entries;
  if (promote.length === 0) return 0;
  const cache = loadCache(cachePath);
  for (const entry of promote) {
    cache.entries[entry.cache_key] = {
      version: CACHE_VERSION,
      actions: entry.actions,
      hit_streak: 0,
      last_recorded_at: entry.staged_at
    };
  }
  persistCache(cachePath, cache);

  // Drop the promoted entries from staging. Keep any others (e.g. when
  // promoting only one flow at a time).
  const remaining = flowFilter ? staging.entries.filter((e) => e.flow !== flowFilter) : [];
  persist(stagingPath, { entries: remaining });
  return promote.length;
}

/** Append a healing artifact event for the CI PR-comment step. */
export function writeHealingArtifact(artifactPath: string, event: HealingArtifactEntry): void {
  const existing = existsSync(artifactPath)
    ? (() => {
        try {
          const parsed = JSON.parse(readFileSync(artifactPath, "utf-8"));
          return Array.isArray(parsed) ? (parsed as HealingArtifactEntry[]) : [];
        } catch {
          return [] as HealingArtifactEntry[];
        }
      })()
    : ([] as HealingArtifactEntry[]);
  existing.push(event);
  mkdirSync(dirname(artifactPath), { recursive: true });
  writeFileSync(artifactPath, JSON.stringify(existing, null, 2), "utf-8");
}

function loadStaging(path: string): HealingStagingFile {
  if (!existsSync(path)) return { entries: [] };
  try {
    const parsed = JSON.parse(readFileSync(path, "utf-8")) as Partial<HealingStagingFile>;
    return { entries: parsed.entries ?? [] };
  } catch {
    return { entries: [] };
  }
}

function persist(path: string, file: HealingStagingFile): void {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(file, null, 2), "utf-8");
}

function loadCache(path: string): CacheFile {
  if (!existsSync(path)) return { entries: {} };
  try {
    const parsed = JSON.parse(readFileSync(path, "utf-8")) as Partial<CacheFile>;
    return { entries: parsed.entries ?? {} };
  } catch {
    return { entries: {} };
  }
}

function persistCache(path: string, file: CacheFile): void {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(file, null, 2), "utf-8");
}
