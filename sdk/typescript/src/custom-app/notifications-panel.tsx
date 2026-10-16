// The open inbox under `<NotificationsBell />`: a header with "Mark all read",
// then the list, newest first, each thread folded into its newest item. Same
// words as Oxygen's own bell.
//
// Positioned `fixed` from the bell's box, so a header with `overflow: hidden`
// cannot clip it. Up to 480px wide the panel spans the screen less an 8px
// gutter; above that it is 360px, lined up with the bell.

import * as React from "react";
import type { NotificationLevel } from "./notification-catalog";
import {
  categoryLabel,
  collapseThreads,
  formatAge,
  groupUnread,
  type NotificationGroup
} from "./notifications";
import type { UseNotificationsResult } from "./notifications-hooks";

const NARROW_MAX = 480;
const GUTTER = 8;
const WIDTH = 360;

const SANS =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';
const T = {
  bg: "var(--oxy-notifications-bg, var(--oxy-shell-background, var(--color-card, #ffffff)))",
  fg: "var(--oxy-notifications-fg, var(--oxy-shell-foreground, var(--color-foreground, #0a0a0a)))",
  muted:
    "var(--oxy-notifications-muted, var(--oxy-shell-muted-fg, var(--color-muted-foreground, #52525b)))",
  border: "var(--oxy-notifications-border, var(--oxy-shell-border, var(--color-border, #e4e4e7)))",
  hover: "var(--oxy-notifications-hover, var(--oxy-shell-accent, var(--color-muted, #f4f4f5)))",
  accent: "var(--oxy-notifications-accent, var(--oxy-shell-link, var(--color-info, #2563eb)))",
  critical:
    "var(--oxy-notifications-critical, var(--oxy-shell-destructive, var(--color-status-error, #dc2626)))",
  timeSensitive: "var(--oxy-notifications-time-sensitive, var(--color-warning, #d97706))",
  z: "var(--oxy-notifications-z, 50)"
};

/** The levels a row marks, with the word a screen reader hears. The rest look alike. */
const LEVEL_ACCENT: Partial<Record<NotificationLevel, { color: string; words: string }>> = {
  critical: { color: T.critical, words: "Critical" },
  time_sensitive: { color: T.timeSensitive, words: "Time-sensitive" }
};

export interface BellPanelProps {
  inbox: UseNotificationsResult;
  anchor: React.RefObject<HTMLButtonElement | null>;
  align: "start" | "end";
  label: string;
  /** Name the app on each row (the whole inbox, not just this app's). */
  showApp: boolean;
  className?: string;
  /** A row was tapped: its newest item, and the thread folded under it. */
  onItem: (group: NotificationGroup) => void;
  /** `refocus`: hand focus back to the bell (Escape), not on a click elsewhere. */
  onClose: (refocus: boolean) => void;
}

export function BellPanel(props: BellPanelProps): React.JSX.Element {
  const { inbox, anchor, align, label } = props;
  const panelRef = React.useRef<HTMLDivElement>(null);
  const place = usePlacement(anchor, align);
  useDismiss(panelRef, anchor, props.onClose);
  React.useEffect(() => panelRef.current?.focus(), []);
  const hasUnread = inbox.unread > 0 || inbox.items.some((i) => i.read_at === null);

  return (
    <div
      ref={panelRef}
      role='dialog'
      aria-label={label}
      tabIndex={-1}
      className={props.className}
      style={{ ...styles.panel, ...place }}
      data-testid='oxy-notifications-panel'
    >
      <div style={styles.header}>
        <span style={styles.heading}>{label}</span>
        {/* A staging or dev host may not change read state: no button to offer. */}
        {inbox.readOnly ? null : (
          <TextButton
            disabled={!hasUnread}
            onClick={() => void inbox.markAllRead()}
            testId='oxy-notifications-mark-all-read'
          >
            Mark all read
          </TextButton>
        )}
      </div>
      <div style={styles.list} data-testid='oxy-notifications-list'>
        <PanelBody {...props} />
      </div>
    </div>
  );
}

function PanelBody({ inbox, showApp, onItem }: BellPanelProps): React.JSX.Element {
  const { items, isLoading, error } = inbox;
  if (items.length === 0 && isLoading) {
    return <Note testId='oxy-notifications-loading'>Loading…</Note>;
  }
  if (items.length === 0 && error) {
    return (
      <div style={styles.note} data-testid='oxy-notifications-error'>
        Couldn't load notifications
        <div style={{ marginTop: 8 }}>
          <TextButton onClick={() => void inbox.refresh()} testId='oxy-notifications-retry'>
            Try again
          </TextButton>
        </div>
      </div>
    );
  }
  if (items.length === 0) {
    return <Note testId='oxy-notifications-empty'>You're all caught up</Note>;
  }
  const now = Date.now();
  return (
    <ul style={styles.ul}>
      {collapseThreads(items).map((group) => (
        <li key={group.item.id} style={styles.li}>
          <Row group={group} now={now} showApp={showApp} onClick={() => onItem(group)} />
        </li>
      ))}
    </ul>
  );
}

function Row(props: {
  group: NotificationGroup;
  now: number;
  showApp: boolean;
  onClick: () => void;
}): React.JSX.Element {
  const { group, now, showApp } = props;
  const { item, more } = group;
  const unread = groupUnread(group);
  const accent = item.level ? LEVEL_ACCENT[item.level] : undefined;
  const category = categoryLabel(item);
  const [hover, setHover] = React.useState(false);
  const time = (
    <time dateTime={item.created_at} title={new Date(item.created_at).toLocaleString()}>
      {formatAge(item.created_at, now)}
    </time>
  );
  return (
    <button
      type='button'
      onClick={props.onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      data-testid={`oxy-notifications-item-${item.id}`}
      data-unread={unread}
      data-level={item.level}
      style={{
        ...styles.row,
        background: hover ? T.hover : "transparent",
        // A bar on the leading edge: inset, so the row does not move.
        boxShadow: accent ? `inset 3px 0 0 ${accent.color}` : undefined
      }}
    >
      <span
        aria-hidden='true'
        style={{ ...styles.dot, visibility: unread ? "visible" : "hidden" }}
      />
      <span style={styles.rowText}>
        {/* The whole inbox names the app first; one app's leads with the title. */}
        {showApp ? (
          <span style={styles.meta}>
            <span style={styles.ellipsis}>{item.app?.name ?? "Oxygen"}</span>
            {time}
          </span>
        ) : null}
        <span style={styles.titleLine}>
          {item.title === null ? (
            <span style={{ ...styles.body, fontStyle: "italic" }}>
              Couldn't load this notification
            </span>
          ) : (
            <span style={{ ...styles.title, fontWeight: unread ? 600 : 400 }}>{item.title}</span>
          )}
          {showApp ? null : <span style={styles.meta}>{time}</span>}
        </span>
        {category ? (
          <span
            style={{ ...styles.category, ...styles.ellipsis }}
            title={category.full}
            data-testid={`oxy-notifications-category-${item.id}`}
          >
            {category.leaf}
          </span>
        ) : null}
        {item.title !== null && item.body ? <span style={styles.body}>{item.body}</span> : null}
        {more.length > 0 ? (
          <span style={styles.category} data-testid={`oxy-notifications-more-${item.id}`}>
            +{more.length} more
          </span>
        ) : null}
        {accent ? <span style={styles.srOnly}>{accent.words}</span> : null}
        {unread ? <span style={styles.srOnly}>Unread</span> : null}
      </span>
    </button>
  );
}

function TextButton(props: {
  onClick: () => void;
  disabled?: boolean;
  testId: string;
  children: React.ReactNode;
}): React.JSX.Element {
  return (
    <button
      type='button'
      onClick={props.onClick}
      disabled={props.disabled}
      data-testid={props.testId}
      style={{ ...styles.textButton, opacity: props.disabled ? 0.5 : 1 }}
    >
      {props.children}
    </button>
  );
}

function Note(props: { testId: string; children: React.ReactNode }): React.JSX.Element {
  return (
    <p style={styles.note} data-testid={props.testId}>
      {props.children}
    </p>
  );
}

// ── Behavior ────────────────────────────────────────────────────────────────

/** `fixed` coordinates from the bell's box, kept current on resize and scroll. */
function usePlacement(
  anchor: React.RefObject<HTMLButtonElement | null>,
  align: "start" | "end"
): React.CSSProperties {
  const measure = React.useCallback((): React.CSSProperties => {
    const box = anchor.current?.getBoundingClientRect();
    if (!box || typeof window === "undefined") return {};
    const vw = window.innerWidth;
    const top = box.bottom + 6;
    const maxHeight = Math.max(200, window.innerHeight - top - GUTTER);
    if (vw <= NARROW_MAX) return { top, left: GUTTER, right: GUTTER, maxHeight };
    const width = Math.min(WIDTH, vw - 2 * GUTTER);
    const left =
      align === "end"
        ? Math.max(GUTTER, box.right - width)
        : Math.min(box.left, vw - width - GUTTER);
    return { top, left, width, maxHeight: Math.min(maxHeight, 480) };
  }, [anchor, align]);
  const [place, setPlace] = React.useState<React.CSSProperties>(measure);
  React.useEffect(() => {
    const update = () => setPlace(measure());
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [measure]);
  return place;
}

/** Close on Escape (focus back to the bell) and on a press outside panel and bell. */
function useDismiss(
  panel: React.RefObject<HTMLDivElement | null>,
  anchor: React.RefObject<HTMLButtonElement | null>,
  onClose: (refocus: boolean) => void
): void {
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose(true);
    };
    const onPress = (e: PointerEvent | MouseEvent) => {
      const t = e.target as Node | null;
      if (!t || panel.current?.contains(t) || anchor.current?.contains(t)) return;
      onClose(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPress);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPress);
    };
  }, [panel, anchor, onClose]);
}

// ── Styles ──────────────────────────────────────────────────────────────────

const styles: Record<string, React.CSSProperties> = {
  panel: {
    position: "fixed",
    zIndex: T.z,
    display: "flex",
    flexDirection: "column",
    boxSizing: "border-box",
    background: T.bg,
    color: T.fg,
    border: `1px solid ${T.border}`,
    borderRadius: 8,
    boxShadow: "0 10px 30px rgba(0, 0, 0, 0.18)",
    fontFamily: SANS,
    fontSize: 13,
    lineHeight: 1.4,
    textAlign: "left",
    overflow: "hidden",
    outline: "none"
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 8,
    padding: "10px 12px",
    borderBottom: `1px solid ${T.border}`,
    flexShrink: 0
  },
  heading: { fontWeight: 600, fontSize: 13 },
  list: { overflowY: "auto", minHeight: 0, flex: "1 1 auto" },
  ul: { listStyle: "none", margin: 0, padding: 0 },
  li: { borderBottom: `1px solid ${T.border}` },
  row: {
    display: "flex",
    alignItems: "flex-start",
    gap: 8,
    width: "100%",
    padding: "10px 12px",
    border: "none",
    color: T.fg,
    font: "inherit",
    textAlign: "left",
    cursor: "pointer"
  },
  dot: {
    flexShrink: 0,
    width: 8,
    height: 8,
    marginTop: 5,
    borderRadius: "50%",
    background: T.accent
  },
  rowText: { display: "flex", flexDirection: "column", gap: 2, minWidth: 0, flex: 1 },
  meta: {
    display: "flex",
    justifyContent: "space-between",
    gap: 8,
    flexShrink: 0,
    fontSize: 11,
    color: T.muted
  },
  ellipsis: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  titleLine: { display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 8 },
  title: { fontSize: 13, overflowWrap: "anywhere", minWidth: 0 },
  category: { fontSize: 11, color: T.muted },
  body: {
    fontSize: 12,
    color: T.muted,
    display: "-webkit-box",
    WebkitLineClamp: 2,
    WebkitBoxOrient: "vertical",
    overflow: "hidden",
    overflowWrap: "anywhere"
  },
  textButton: {
    border: "none",
    background: "transparent",
    padding: "2px 4px",
    color: T.muted,
    font: "inherit",
    fontSize: 12,
    cursor: "pointer"
  },
  note: { margin: 0, padding: "24px 12px", textAlign: "center", color: T.muted, fontSize: 13 },
  srOnly: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clip: "rect(0 0 0 0)",
    whiteSpace: "nowrap"
  }
};
