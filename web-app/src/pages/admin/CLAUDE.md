# Admin panel conventions

Everything under `src/pages/admin/` is an operator surface: dense, scannable, read
by people who keep many rows on screen at once. It is deliberately smaller and
tighter than the customer-facing product.

## Type scale (HARD — no exceptions)

| Role | Class |
| ---- | ----- |
| Page title (`h1`) | `text-xl font-semibold tracking-tight` |
| Card / section heading (`h3`) | `text-sm font-semibold` |
| Collapsible section label | `text-[10px] uppercase tracking-[0.16em]` |
| **Body, table cells, empty states, help text** | `text-xs` |
| Metric value | `text-sm tabular-nums` (hero metric: `text-2xl`) |

**`text-xs` is the default.** Reach for anything larger only from the table above.
`text-base` and `text-lg` do not appear in the admin panel at all — if a size
feels too small, the fix is weight or color (`font-medium`, `text-foreground` vs
`text-muted-foreground`), not points.

**One exception, and only one:** a monogram glyph sized to its avatar box
(`OrgLogo`'s `lg: "size-16 text-2xl"`). That is artwork filling a fixed square,
not type — leave it alone, and don't let a scale sweep "fix" it.

Icons follow the text: `size-3` beside `text-xs`, `size-3.5` in a heading row.

## Naming & targetability

An operator pointing at a bug should be able to name the component from the DOM.

- Every section, list, row, empty state, and stat carries a `data-testid` of the
  form **`admin-<area>-<element>`**, kebab-case
  (`admin-app-activity-visitors-empty`, `admin-app-dossier-section-functions`).
- **Key the testid off a stable id, not display copy.** `DossierSection` takes an
  `id: SectionId` for exactly this reason — the title is editable prose, the id
  is not.
- Sub-components get their own named file once a file passes ~150 lines. Four
  anonymous inner components in one 215-line file is what made `Activity`
  untargetable; it is now `Activity/components/Activity{Summary,Visitors,Events,Stat}.tsx`.
- Name a component for what it *is* on screen (`ActivityVisitors`), not for its
  position in a layout (`Section2`).
