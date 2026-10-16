// The notification catalogue an app declares in `oxy-app.json` (the app-level
// `notifications` block): its types, the checks publish makes on it, and how a
// kind resolves to what the inbox shows (category, label, level, actions).
//
// ONE SOURCE, THREE PACKAGES. This file lives in `sdk/typescript`; byte copies
// sit in `sdk/cli/src/publish/` (`oxyc validate`, `oxyc publish`) and
// `sdk/vite-plugin/src/` (`pnpm dev`, `pnpm build`), because neither of those
// packages depends on another. Edit this one, then run
// `node sdk/typescript/scripts/sync-notification-catalog.mjs`; a test in each
// copy's package fails while the bytes differ.
//
// THE SERVER IS THE AUTHORITY. Every rule repeats the publish check in
// `notifications/catalog/validate.rs`, and `resolveNotificationKind` repeats
// the lenient read the inbox uses (`catalog/parse.rs`, `catalog/source.rs`).
// The one difference is on purpose: the server stops at the first problem,
// this lists every one, so an author fixes them in one pass.
//
// No imports and nothing past ES2020, so it compiles under all three packages'
// settings.

/** How hard a notification asks for attention. Only `critical` ignores mutes. */
export type NotificationLevel = "passive" | "active" | "time_sensitive" | "critical";

/** One category, keyed by its path (`"operations/food-safety"`). */
export interface OxyAppNotificationCategory {
  /** What people see, ≤ 64 characters. Default: the last segment, humanized. */
  label?: string;
  /** ≤ 200 characters. */
  description?: string;
}

/** A button on a notification. */
export interface OxyAppNotificationAction {
  /** Unique within the kind: 1–32 lowercase letters, digits, `_` or `-` (`ack`, `open`). */
  id: string;
  /** The button's text, ≤ 24 characters. */
  label: string;
  /**
   * A function this app declares. Tapping calls it as the signed-in person with
   * `{ notificationId, action, link }`, then marks the notification read.
   * Absent: the button opens the notification's link.
   */
  function?: string;
  /** Shown as a destructive action (red on a phone). */
  destructive?: boolean;
}

/** One kind in the catalogue, keyed by the kind a function sends. */
export interface OxyAppNotificationKind {
  /** A declared category path, a parent of one, or `general` (the default). */
  category?: string;
  /** What people see, and mute by, ≤ 64 characters. Default: the kind, humanized. */
  label?: string;
  /** ≤ 200 characters. */
  description?: string;
  /**
   * Default `active`. A function declaring the kind `critical: true` makes it
   * `critical` whatever this says.
   */
  level?: NotificationLevel;
  /** At most 3. */
  actions?: OxyAppNotificationAction[];
}

/**
 * The app-level `notifications` block: the catalogue that labels, groups and
 * ranks the kinds this app's functions send. Which kinds a function may send is
 * still its own `notify.kinds`; this block only describes them.
 *
 * ```json
 * "notifications": {
 *   "categories": { "operations/food-safety": { "label": "Food safety" } },
 *   "kinds": {
 *     "temp_alarm": {
 *       "category": "operations/food-safety",
 *       "label": "Temperature alarm",
 *       "level": "critical",
 *       "actions": [{ "id": "ack", "label": "Acknowledge", "function": "ack-alarm" }]
 *     }
 *   }
 * }
 * ```
 */
export interface OxyAppNotificationsManifest {
  categories?: Record<string, OxyAppNotificationCategory>;
  kinds?: Record<string, OxyAppNotificationKind>;
}

/** Every level, least to most urgent. */
export const NOTIFICATION_LEVELS: readonly NotificationLevel[] = [
  "passive",
  "active",
  "time_sensitive",
  "critical"
];

/** One segment of a category path, and an action's `id`. */
export const CATALOG_SEGMENT = /^[a-z0-9][a-z0-9_-]{0,31}$/;
/** Segments in a category path, at most. */
export const MAX_CATEGORY_DEPTH = 3;
/** Actions on one kind, at most: what a lock-screen notification shows. */
export const MAX_NOTIFICATION_ACTIONS = 3;
/** An action's `label`, in characters: a button on a phone. */
export const MAX_ACTION_LABEL_CHARS = 24;
/** A category's or a kind's `label`, in characters. */
export const MAX_CATALOG_LABEL_CHARS = 64;
/** A category's or a kind's `description`, in characters. */
export const MAX_CATALOG_DESCRIPTION_CHARS = 200;
/** Kinds an app may not declare or send: kept for Oxygen's own notices. */
export const RESERVED_NOTIFY_KIND_PREFIX = "oxy.";
/** The category of a kind that names none. */
export const GENERAL_CATEGORY = "general";
const GENERAL_LABEL = "General";

/** `valid_kind` in `notifications/input.rs`: what a person mutes by. */
const KIND = /^[a-z0-9_.-]{1,64}$/;

/** The keys each object of the block may have; any other is refused. */
const FIELDS = {
  block: ["categories", "kinds"],
  category: ["label", "description"],
  kind: ["category", "label", "description", "level", "actions"],
  action: ["id", "label", "function", "destructive"]
};

/** One thing wrong with the catalogue. Errors refuse a publish; warnings do not. */
export interface CatalogFinding {
  level: "error" | "warning";
  /** Where, in `oxy-app.json`: `notifications.kinds.temp_alarm.level`. */
  path: string;
  message: string;
}

/** A notification's category as the inbox sends it. */
export interface NotificationCategoryRef {
  /** `"operations/food-safety"`; `"general"` for a kind that names none. */
  path: string;
  /** Root to leaf: `["Store operations", "Food safety"]`. */
  labels: string[];
}

/** A notification's action as the inbox sends it. */
export interface NotificationAction {
  id: string;
  label: string;
  destructive: boolean;
  /** True when tapping it calls one of the app's functions; false: it opens the link. */
  runs_function: boolean;
}

/** What the inbox shows for one kind, from the catalogue and its defaults. */
export interface ResolvedNotificationKind {
  category: NotificationCategoryRef;
  kind_label: string;
  level: NotificationLevel;
  actions: NotificationAction[];
}

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** An own key — `Object.keys`, so `__proto__` and `constructor` are not entries. */
function own(map: Json, key: string): unknown {
  return Object.keys(map).indexOf(key) >= 0 ? map[key] : undefined;
}

/** Characters as the server counts them: code points, not UTF-16 units. */
function chars(s: string): number {
  return Array.from(s).length;
}

/** A trimmed, non-blank string, else `undefined` — `text` in `catalog/parse.rs`. */
function textOf(value: unknown): string | undefined {
  const s = typeof value === "string" ? value.trim() : "";
  return s ? s : undefined;
}

/**
 * A label for a name nobody labelled: `"list_overdue"` → `"List overdue"`,
 * `"food-safety"` → `"Food safety"` — `humanize` in `catalog/model.rs`.
 */
export function humanizeNotificationName(name: string): string {
  const words = name
    .replace(/[_.-]/g, " ")
    .split(/\s+/)
    .filter((w) => w)
    .join(" ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A kind a function may send: 1–64 lowercase letters, digits, `_`, `-`, `.`. */
export function isValidNotifyKind(kind: string): boolean {
  return KIND.test(kind);
}

/** Every segment well formed, ignoring depth. */
function wellFormedPath(path: string): boolean {
  return path.split("/").every((segment) => CATALOG_SEGMENT.test(segment));
}

/** A category path a manifest may declare: well formed and at most 3 deep. */
export function isValidCategoryPath(path: string): boolean {
  return wellFormedPath(path) && path.split("/").length <= MAX_CATEGORY_DEPTH;
}

/** `"a/b/c"` → `["a", "a/b", "a/b/c"]`. */
function ancestors(path: string): string[] {
  const segments = path.split("/");
  return segments.map((_, i) => segments.slice(0, i + 1).join("/"));
}

/** Each kind the functions may send (`functions.*.notify.kinds`) → whether any marks it critical. */
function declaredKinds(functions: Json): Record<string, boolean> {
  const out: Record<string, boolean> = {};
  for (const name of Object.keys(functions)) {
    const spec = functions[name];
    const notify = isObject(spec) ? spec.notify : undefined;
    const kinds = isObject(notify) && isObject(notify.kinds) ? notify.kinds : {};
    for (const kind of Object.keys(kinds)) {
      const entry = kinds[kind];
      out[kind] = own(out, kind) === true || (isObject(entry) && entry.critical === true);
    }
  }
  return out;
}

/** The manifest's parts this file reads, each defaulted to empty. */
function partsOf(manifest: unknown): { functions: Json; block: unknown } {
  const m = isObject(manifest) ? manifest : {};
  return { functions: isObject(m.functions) ? m.functions : {}, block: m.notifications };
}

class Findings {
  readonly list: CatalogFinding[] = [];
  error(path: string, message: string): void {
    this.list.push({ level: "error", path, message });
  }
  warning(path: string, message: string): void {
    this.list.push({ level: "warning", path, message });
  }
  /** `only_keys`: every key of `obj` not in `allowed`, each refused. */
  onlyKeys(obj: Json, at: string, allowed: string[]): void {
    for (const key of Object.keys(obj)) {
      if (allowed.indexOf(key) >= 0) continue;
      this.error(
        `${at}.${key}`,
        `\`${at}.${key}\` is not a field; ${at} takes ${allowed.join(", ")}.`
      );
    }
  }
  /** `bounded_text`: absent, or a non-blank string of at most `max` characters. */
  text(at: string, value: unknown, max: number): void {
    if (value === undefined) return;
    if (typeof value === "string" && value.trim() && chars(value) <= max) return;
    this.error(at, `\`${at}\` must be a non-empty string of at most ${max} characters.`);
  }
}

/** Function-level `notify.kinds` in Oxygen's reserved namespace (publish's `notify_kind_reserved`). */
function checkReservedFunctionKinds(functions: Json, out: Findings): void {
  for (const name of Object.keys(functions)) {
    const spec = functions[name];
    const notify = isObject(spec) ? spec.notify : undefined;
    const kinds = isObject(notify) && isObject(notify.kinds) ? notify.kinds : {};
    for (const kind of Object.keys(kinds)) {
      if (!kind.startsWith(RESERVED_NOTIFY_KIND_PREFIX)) continue;
      out.error(
        `functions.${name}.notify.kinds.${kind}`,
        `function "${name}" declares notify kind "${kind}", which starts with ` +
          `"${RESERVED_NOTIFY_KIND_PREFIX}" — reserved for Oxygen's own notices; rename it.`
      );
    }
  }
}

/** Why `path` is refused as a category path, or `null`. Malformed before too deep. */
function pathProblem(path: string): string | null {
  const name = JSON.stringify(path);
  if (!wellFormedPath(path)) {
    return (
      `Category ${name} is not a valid path: each segment is 1–32 lowercase letters, ` +
      'digits, "_" or "-", starting with a letter or digit, joined by "/".'
    );
  }
  const depth = path.split("/").length;
  if (depth > MAX_CATEGORY_DEPTH) {
    return `Category ${name} is ${depth} levels deep; at most ${MAX_CATEGORY_DEPTH} are allowed.`;
  }
  return null;
}

/** The declared categories; returns every path a kind may name (declared, implied, `general`). */
function checkCategories(raw: unknown, out: Findings): Set<string> {
  const known = new Set<string>([GENERAL_CATEGORY]);
  if (raw === undefined) return known;
  if (!isObject(raw)) {
    out.error(
      "notifications.categories",
      "`notifications.categories` must be an object keyed by category path."
    );
    return known;
  }
  for (const path of Object.keys(raw)) {
    const at = `notifications.categories.${path}`;
    const problem = pathProblem(path);
    if (problem) {
      out.error(at, problem);
      continue;
    }
    for (const p of ancestors(path)) known.add(p);
    const entry = raw[path];
    if (!isObject(entry)) {
      out.error(at, `\`${at}\` must be an object.`);
      continue;
    }
    out.onlyKeys(entry, at, FIELDS.category);
    out.text(`${at}.label`, entry.label, MAX_CATALOG_LABEL_CHARS);
    out.text(`${at}.description`, entry.description, MAX_CATALOG_DESCRIPTION_CHARS);
  }
  return known;
}

interface KindContext {
  known: Set<string>;
  functions: Json;
  declared: Record<string, boolean>;
}

function checkCategoryRef(
  kind: string,
  value: unknown,
  at: string,
  ctx: KindContext,
  out: Findings
): void {
  if (value === undefined) return;
  if (typeof value !== "string") {
    out.error(`${at}.category`, `\`${at}.category\` must be a category path.`);
    return;
  }
  const problem = pathProblem(value);
  if (problem) {
    out.error(`${at}.category`, problem);
  } else if (!ctx.known.has(value)) {
    out.error(
      `${at}.category`,
      `Kind ${JSON.stringify(kind)} names the category ${JSON.stringify(value)}, which ` +
        "notifications.categories does not declare."
    );
  }
}

/** One action; returns its id when it has a valid one. */
function checkAction(
  kind: string,
  raw: unknown,
  at: string,
  ctx: KindContext,
  out: Findings
): string | null {
  const invalid = (why: string) =>
    out.error(at, `Action \`${at}\` of kind ${JSON.stringify(kind)} ${why}.`);
  if (!isObject(raw)) {
    invalid("must be an object { id, label, function?, destructive? }");
    return null;
  }
  out.onlyKeys(raw, at, FIELDS.action);
  const id = typeof raw.id === "string" && CATALOG_SEGMENT.test(raw.id) ? raw.id : null;
  if (id === null) invalid('needs an `id` of 1–32 lowercase letters, digits, "_" or "-"');
  const label = raw.label;
  if (typeof label !== "string" || !label.trim() || chars(label) > MAX_ACTION_LABEL_CHARS) {
    invalid(`needs a \`label\` of 1–${MAX_ACTION_LABEL_CHARS} characters`);
  }
  if (raw.destructive !== undefined && typeof raw.destructive !== "boolean") {
    invalid("has a `destructive` that is not true or false");
  }
  const fn = raw.function;
  if (fn !== undefined && (typeof fn !== "string" || own(ctx.functions, fn) === undefined)) {
    out.error(
      `${at}.function`,
      `Action ${JSON.stringify(id ?? raw.id)} of kind ${JSON.stringify(kind)} calls the function ` +
        `${JSON.stringify(fn)}, which this manifest does not declare under functions.`
    );
  }
  return id;
}

function checkActions(
  kind: string,
  raw: unknown,
  at: string,
  ctx: KindContext,
  out: Findings
): void {
  if (raw === undefined) return;
  if (!Array.isArray(raw)) {
    out.error(`${at}.actions`, `\`${at}.actions\` must be a list.`);
    return;
  }
  if (raw.length > MAX_NOTIFICATION_ACTIONS) {
    out.error(
      `${at}.actions`,
      `Kind ${JSON.stringify(kind)} declares ${raw.length} actions; at most ` +
        `${MAX_NOTIFICATION_ACTIONS} are allowed.`
    );
  }
  const ids = new Set<string>();
  for (let i = 0; i < raw.length; i++) {
    const id = checkAction(kind, raw[i], `${at}.actions[${i}]`, ctx, out);
    if (id === null) continue;
    if (ids.has(id)) {
      out.error(
        `${at}.actions[${i}].id`,
        `Kind ${JSON.stringify(kind)} declares the action ${JSON.stringify(id)} twice.`
      );
    }
    ids.add(id);
  }
}

/** `warn_kind`: a kind nothing sends, and a level a function's `critical: true` overrides. */
function warnKind(kind: string, level: unknown, at: string, ctx: KindContext, out: Findings): void {
  const critical = own(ctx.declared, kind);
  if (critical === undefined) {
    out.warning(at, "no function declares it under notify.kinds, so nothing can send it yet");
  } else if (critical === true && typeof level === "string" && level !== "critical") {
    out.warning(
      at,
      `its level is ${level} but a function marks it critical: true; it is delivered as critical`
    );
  }
}

function checkKind(kind: string, raw: unknown, ctx: KindContext, out: Findings): void {
  const at = `notifications.kinds.${kind}`;
  const name = JSON.stringify(kind);
  if (kind.startsWith(RESERVED_NOTIFY_KIND_PREFIX)) {
    out.error(
      at,
      `Kind ${name} starts with "${RESERVED_NOTIFY_KIND_PREFIX}", which is reserved for ` +
        "Oxygen's own notices."
    );
    return;
  }
  if (!isValidNotifyKind(kind)) {
    out.error(at, `Kind ${name} must be 1–64 lowercase letters, digits, "_", "-" or ".".`);
    return;
  }
  if (!isObject(raw)) {
    out.error(at, `\`${at}\` must be an object.`);
    return;
  }
  out.onlyKeys(raw, at, FIELDS.kind);
  out.text(`${at}.label`, raw.label, MAX_CATALOG_LABEL_CHARS);
  out.text(`${at}.description`, raw.description, MAX_CATALOG_DESCRIPTION_CHARS);
  checkCategoryRef(kind, raw.category, at, ctx, out);
  const level = raw.level;
  const levelOk =
    level === undefined || NOTIFICATION_LEVELS.indexOf(level as NotificationLevel) >= 0;
  if (!levelOk) {
    out.error(
      `${at}.level`,
      `Kind ${name} has level ${JSON.stringify(level)}; expected one of ` +
        `${NOTIFICATION_LEVELS.join(", ")}.`
    );
  }
  checkActions(kind, raw.actions, at, ctx, out);
  warnKind(kind, levelOk ? level : undefined, at, ctx, out);
}

/**
 * Everything wrong with a manifest's notifications, in manifest order: the
 * `notifications` catalogue (`notifications.*` paths), and any function
 * declaring a kind in Oxygen's namespace (`functions.*` paths). An empty list:
 * nothing to say. Never throws, whatever `manifest` is.
 */
export function checkNotifications(manifest: unknown): CatalogFinding[] {
  const out = new Findings();
  const { functions, block } = partsOf(manifest);
  checkReservedFunctionKinds(functions, out);
  if (block === undefined) return out.list;
  if (!isObject(block)) {
    out.error("notifications", "`notifications` must be an object with `categories` and `kinds`.");
    return out.list;
  }
  out.onlyKeys(block, "notifications", FIELDS.block);
  const known = checkCategories(block.categories, out);
  const kinds = block.kinds;
  if (kinds === undefined) return out.list;
  if (!isObject(kinds)) {
    out.error("notifications.kinds", "`notifications.kinds` must be an object keyed by kind.");
    return out.list;
  }
  const ctx: KindContext = { known, functions, declared: declaredKinds(functions) };
  for (const kind of Object.keys(kinds)) checkKind(kind, kinds[kind], ctx, out);
  return out.list;
}

// ── The lenient read: what the inbox shows ──────────────────────────────────

/** The well-formed categories' labels, by path. */
function categoryLabels(raw: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!isObject(raw)) return out;
  for (const path of Object.keys(raw)) {
    const entry = raw[path];
    const label = isObject(entry) ? textOf(entry.label) : undefined;
    if (label && isValidCategoryPath(path)) out[path] = label;
  }
  return out;
}

/** `label_of`, root to leaf. */
function categoryRef(labels: Record<string, string>, path: string): NotificationCategoryRef {
  return {
    path,
    labels: ancestors(path).map((p) => {
      const declared = own(labels, p);
      if (typeof declared === "string") return declared;
      if (p === GENERAL_CATEGORY) return GENERAL_LABEL;
      return humanizeNotificationName(p.slice(p.lastIndexOf("/") + 1));
    })
  };
}

/** The well-formed actions, at most 3 — `action` in `catalog/parse.rs`. */
function actionRefs(raw: unknown): NotificationAction[] {
  if (!Array.isArray(raw)) return [];
  const out: NotificationAction[] = [];
  for (const a of raw) {
    if (!isObject(a) || typeof a.id !== "string" || !CATALOG_SEGMENT.test(a.id)) continue;
    const label = textOf(a.label);
    if (!label) continue;
    out.push({
      id: a.id,
      label,
      destructive: a.destructive === true,
      runs_function: textOf(a.function) !== undefined
    });
  }
  return out.slice(0, MAX_NOTIFICATION_ACTIONS);
}

/**
 * What the inbox shows for `kind`: its catalogue entry where the manifest has
 * one, and the defaults where it does not — category `general`, the kind
 * humanized as its label, level `active`, no actions. A function declaring the
 * kind `critical: true` makes it `critical` whatever the entry says. Lenient,
 * like the server's read of a live build: a malformed part falls back to its
 * default rather than throwing.
 */
export function resolveNotificationKind(manifest: unknown, kind: string): ResolvedNotificationKind {
  const { functions, block } = partsOf(manifest);
  const catalogue = isObject(block) ? block : {};
  const kinds = isObject(catalogue.kinds) ? catalogue.kinds : {};
  const raw =
    isValidNotifyKind(kind) && !kind.startsWith(RESERVED_NOTIFY_KIND_PREFIX)
      ? own(kinds, kind)
      : undefined;
  const entry = isObject(raw) ? raw : {};
  const category = textOf(entry.category);
  const path = category && isValidCategoryPath(category) ? category : GENERAL_CATEGORY;
  const declared = NOTIFICATION_LEVELS.indexOf(entry.level as NotificationLevel) >= 0;
  const level: NotificationLevel =
    own(declaredKinds(functions), kind) === true
      ? "critical"
      : declared
        ? (entry.level as NotificationLevel)
        : "active";
  return {
    category: categoryRef(categoryLabels(catalogue.categories), path),
    kind_label: textOf(entry.label) ?? humanizeNotificationName(kind),
    level,
    actions: actionRefs(entry.actions)
  };
}
