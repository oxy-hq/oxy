import type {
  CategoryNode,
  KindNode,
  NotificationChannel,
  SourceNode
} from "@/types/notifications";

/** The channels a person can mute, in the order the switches show them. */
export const CHANNELS: { value: NotificationChannel; label: string }[] = [
  { value: "push", label: "Push" },
  { value: "email", label: "Email" },
  { value: "slack", label: "Slack" }
];

/** How many devices push reaches, in a few words. */
export function deviceCount(n: number): string {
  if (n <= 0) return "Not on any device yet";
  return n === 1 ? "Reaches 1 device" : `Reaches ${n} devices`;
}

/** What a switch writes: a whole source or a category (`scope`), or one kind. */
export type PrefTarget = { scope: string } | { kind: string };

/** One row of a source's settings: the source itself, a category, or a kind. */
export interface PrefRow {
  /** Unique within the source; names the row's test ids. */
  key: string;
  label: string;
  description: string | null;
  /** 0 for the source, 1 for a top-level category, and so on. */
  depth: number;
  target: PrefTarget;
  /** Mutes set at this row itself. */
  muted: NotificationChannel[];
  /** The channels anything under this row can be delivered on: the only switches drawn. */
  channels: NotificationChannel[];
  /**
   * A critical kind, or a subtree of nothing but critical kinds: always
   * delivered, so its switches are locked on.
   */
  critical: boolean;
  /** Per channel, the label of the highest row above that mutes it. */
  mutedBy: Partial<Record<NotificationChannel, string>>;
}

interface Ancestor {
  label: string;
  muted: NotificationChannel[];
}

function kindsUnder(node: CategoryNode): KindNode[] {
  return [...node.kinds, ...node.categories.flatMap(kindsUnder)];
}

/** The union of `kinds`' channels, in switch order. */
function channelsOf(kinds: KindNode[]): NotificationChannel[] {
  const all = new Set(kinds.flatMap((k) => k.channels));
  return CHANNELS.map((c) => c.value).filter((c) => all.has(c));
}

function allCritical(kinds: KindNode[]): boolean {
  return kinds.length > 0 && kinds.every((k) => k.critical);
}

/** The highest ancestor that mutes each channel: the one that must be turned back on first. */
function mutedBy(ancestors: Ancestor[]): PrefRow["mutedBy"] {
  const out: PrefRow["mutedBy"] = {};
  for (const { value } of CHANNELS) {
    const by = ancestors.find((a) => a.muted.includes(value));
    if (by) out[value] = by.label;
  }
  return out;
}

function categoryRows(node: CategoryNode, depth: number, ancestors: Ancestor[]): PrefRow[] {
  const kinds = kindsUnder(node);
  const row: PrefRow = {
    key: `cat-${node.path}`,
    label: node.label,
    description: node.description,
    depth,
    target: { scope: node.path },
    muted: node.muted,
    channels: channelsOf(kinds),
    critical: allCritical(kinds),
    mutedBy: mutedBy(ancestors)
  };
  const below = [...ancestors, { label: node.label, muted: node.muted }];
  return [
    row,
    ...node.kinds.map(
      (kind): PrefRow => ({
        key: `kind-${kind.kind}`,
        label: kind.label,
        description: kind.description,
        depth: depth + 1,
        target: { kind: kind.kind },
        muted: kind.muted,
        channels: kind.channels,
        critical: kind.critical,
        mutedBy: mutedBy(below)
      })
    ),
    ...node.categories.flatMap((child) => categoryRows(child, depth + 1, below))
  ];
}

/**
 * A source's tree as the rows Settings draws, top down: the source, then each
 * category followed by its kinds and its own categories. Every row knows which
 * row above it, if any, already mutes each channel.
 */
export function prefRows(source: SourceNode): PrefRow[] {
  const kinds = source.categories.flatMap(kindsUnder);
  const top: PrefRow = {
    key: "source",
    label: source.label,
    description: null,
    depth: 0,
    target: { scope: "" },
    muted: source.muted,
    channels: channelsOf(kinds),
    critical: allCritical(kinds),
    mutedBy: {}
  };
  const ancestors = [{ label: source.label, muted: source.muted }];
  return [top, ...source.categories.flatMap((c) => categoryRows(c, 1, ancestors))];
}
