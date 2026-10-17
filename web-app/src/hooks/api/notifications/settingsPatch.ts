import type {
  GlobalNotificationSettings,
  NodePatch,
  NodeSettings,
  NotificationChannel,
  NotificationSettings,
  NotificationSettingsPatch,
  SettingsCategory,
  SettingsKind,
  SettingsSource
} from "@/types/notifications";

/**
 * A settings patch applied to the document in the cache, the way the server
 * applies it: what the optimistic update shows, what undoes it, and what a
 * 412 is re-checked against.
 */

/** A node's own settings once `node` is applied: `null` takes a channel back to inheriting. */
function withNode(settings: NodeSettings, node: NodePatch): NodeSettings {
  const channels: NodeSettings["channels"] = { ...settings.channels };
  for (const [channel, value] of Object.entries(node.channels ?? {})) {
    if (value === undefined) continue;
    const c = channel as NotificationChannel;
    if (value === null) {
      channels[c] = undefined;
    } else {
      channels[c] = value;
    }
  }
  const explicit = Object.fromEntries(
    Object.entries(channels).filter(([, v]) => v !== undefined)
  ) as NodeSettings["channels"];
  const min_level = node.min_level === undefined ? settings.min_level : node.min_level;
  return { channels: explicit, min_level };
}

const sourceId = (source: SettingsSource) => source.app?.id ?? null;

function patchCategories(categories: SettingsCategory[], node: NodePatch): SettingsCategory[] {
  return categories.map((category) => {
    const own =
      node.scope !== undefined && node.scope === category.path
        ? { ...category, settings: withNode(category.settings, node) }
        : category;
    return {
      ...own,
      kinds:
        node.kind === undefined
          ? own.kinds
          : own.kinds.map((k) =>
              k.kind === node.kind ? { ...k, settings: withNode(k.settings, node) } : k
            ),
      categories: patchCategories(own.categories, node)
    };
  });
}

function patchSource(source: SettingsSource, node: NodePatch): SettingsSource {
  if (sourceId(source) !== node.app_id) return source;
  if (node.scope === "") return { ...source, settings: withNode(source.settings, node) };
  return { ...source, categories: patchCategories(source.categories, node) };
}

/** `doc` with `patch` applied. Nodes it does not find are left alone. */
export function applySettingsPatch(
  doc: NotificationSettings,
  patch: NotificationSettingsPatch
): NotificationSettings {
  const nodes = patch.nodes ?? [];
  return {
    ...doc,
    global: patch.global ? { ...doc.global, ...patch.global } : doc.global,
    sources: doc.sources.map((source) => nodes.reduce(patchSource, source))
  };
}

function kindsUnder(categories: SettingsCategory[]): SettingsKind[] {
  return categories.flatMap((c) => [...c.kinds, ...kindsUnder(c.categories)]);
}

function findCategory(categories: SettingsCategory[], path: string): SettingsCategory | null {
  for (const c of categories) {
    if (c.path === path) return c;
    const below = findCategory(c.categories, path);
    if (below) return below;
  }
  return null;
}

/** A node of `doc`: its own settings, the channels under it, and whether it is all critical. */
interface Located {
  settings: NodeSettings;
  channels: NotificationChannel[];
  criticalOnly: boolean;
}

function locate(doc: NotificationSettings, node: NodePatch): Located | null {
  const source = doc.sources.find((s) => sourceId(s) === node.app_id);
  if (!source) return null;
  const summary = (settings: NodeSettings, kinds: SettingsKind[]): Located => ({
    settings,
    channels: [...new Set(kinds.flatMap((k) => k.channels))],
    criticalOnly: kinds.length > 0 && kinds.every((k) => k.critical)
  });
  if (node.kind !== undefined) {
    const kind = kindsUnder(source.categories).find((k) => k.kind === node.kind);
    return kind ? summary(kind.settings, [kind]) : null;
  }
  if (node.scope === "") return summary(source.settings, kindsUnder(source.categories));
  if (node.scope === undefined) return null;
  const category = findCategory(source.categories, node.scope);
  return category ? summary(category.settings, kindsUnder([category])) : null;
}

/** The patch that puts back what `patch` is about to change in `doc`. */
export function invertSettingsPatch(
  doc: NotificationSettings,
  patch: NotificationSettingsPatch
): NotificationSettingsPatch {
  const global = patch.global
    ? (Object.fromEntries(
        Object.keys(patch.global).map((k) => [k, doc.global[k as keyof GlobalNotificationSettings]])
      ) as Partial<GlobalNotificationSettings>)
    : undefined;
  const nodes = (patch.nodes ?? []).flatMap((node): NodePatch[] => {
    const before = locate(doc, node)?.settings;
    if (!before) return [];
    const { channels, min_level, ...where } = node;
    return [
      {
        ...where,
        ...(channels && {
          channels: Object.fromEntries(
            Object.keys(channels).map((c) => [c, before.channels[c as NotificationChannel] ?? null])
          )
        }),
        ...(min_level !== undefined && { min_level: before.min_level })
      }
    ];
  });
  return { ...(global && { global }), ...(nodes.length > 0 && { nodes }) };
}

/**
 * Whether `patch` still means something against `doc`: every node exists,
 * every channel it sets is one the node can be sent on, and it turns nothing
 * off on a node of only critical kinds. Settings of the person always apply.
 */
export function settingsPatchApplies(
  doc: NotificationSettings,
  patch: NotificationSettingsPatch
): boolean {
  return (patch.nodes ?? []).every((node) => {
    const found = locate(doc, node);
    if (!found) return false;
    return Object.entries(node.channels ?? {}).every(
      ([channel, value]) =>
        found.channels.includes(channel as NotificationChannel) &&
        !(found.criticalOnly && value === false)
    );
  });
}

/** Whether applying `patch` to `doc` would change anything. */
export function settingsPatchChanges(
  doc: NotificationSettings,
  patch: NotificationSettingsPatch
): boolean {
  const after = applySettingsPatch(doc, patch);
  return (
    JSON.stringify([after.global, after.sources]) !== JSON.stringify([doc.global, doc.sources])
  );
}
