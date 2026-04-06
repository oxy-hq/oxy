# React View Transitions

**Version 1.0.0**
Vercel

Animate between UI states using the browser's native `document.startViewTransition`. Declare *what* with `<ViewTransition>`, trigger *when* with `startTransition` / `useDeferredValue` / `Suspense`, control *how* with CSS classes. Unsupported browsers skip animations gracefully.

## When to Animate

Every `<ViewTransition>` should communicate a spatial relationship or continuity. If you can't articulate what it communicates, don't add it.

Implement **all** applicable patterns from this list, in this order:

| Priority | Pattern | What it communicates |
|----------|---------|---------------------|
| 1 | **Shared element** (`name`) | "Same thing — going deeper" |
| 2 | **Suspense reveal** | "Data loaded" |
| 3 | **List identity** (per-item `key`) | "Same items, new arrangement" |
| 4 | **State change** (`enter`/`exit`) | "Something appeared/disappeared" |
| 5 | **Route change** (layout-level) | "Going to a new place" |

This is an implementation order, not a "pick one" list. Most apps need #1–#3 at minimum. Only one tree level should animate at a time.

### Choosing Animation Style

| Context | Animation | Why |
|---------|-----------|-----|
| Hierarchical navigation (list → detail) | Type-keyed `nav-forward` / `nav-back` | Communicates spatial depth |
| Lateral navigation (tab-to-tab) | Bare `<ViewTransition>` (fade) or `default="none"` | No depth to communicate |
| Suspense reveal | `enter`/`exit` string props | Content arriving |
| Revalidation / background refresh | `default="none"` | Silent — no animation needed |

Reserve directional slides for hierarchical navigation only.

---

## Availability

- Requires `react@canary` or `react@experimental` — **not** in stable React (including 19.x). Verify with `npm ls react`.
- Browser support: Chromium 111+, Firefox 144+, Safari 18.2+. Graceful degradation.

---

## Core Concepts

### The `<ViewTransition>` Component

```jsx
import { ViewTransition } from 'react';

<ViewTransition>
  <Component />
</ViewTransition>
```

React auto-assigns a unique `view-transition-name` and calls `document.startViewTransition` behind the scenes. Never call `startViewTransition` yourself.

### Animation Triggers

| Trigger | When it fires |
|---------|--------------|
| **enter** | VT first inserted during a Transition |
| **exit** | VT first removed during a Transition |
| **update** | DOM mutations inside a VT. With nested VTs, mutation applies to the innermost one |
| **share** | Named VT unmounts and another with same `name` mounts in same Transition |

Only `startTransition`, `useDeferredValue`, or `Suspense` activate VTs. Regular `setState` does not animate.

### Critical Placement Rule

VT only activates enter/exit if it appears **before any DOM nodes**:

```jsx
// Works
<ViewTransition enter="auto" exit="auto"><div>Content</div></ViewTransition>

// Broken — div wraps the VT
<div><ViewTransition enter="auto" exit="auto"><div>Content</div></ViewTransition></div>
```

---

## Styling with View Transition Classes

Values: `"auto"` (browser cross-fade), `"none"` (disabled), `"class-name"` (custom CSS), or `{ [type]: value }` for type-specific animations.

```jsx
<ViewTransition default="none" enter="slide-in" exit="slide-out" share="morph" />
```

If `default` is `"none"`, all triggers are off unless explicitly listed.

### CSS Pseudo-Elements

- `::view-transition-old(.class)` — outgoing snapshot
- `::view-transition-new(.class)` — incoming snapshot
- `::view-transition-group(.class)` — container
- `::view-transition-image-pair(.class)` — old + new pair

---

## Transition Types

Tag transitions with `addTransitionType` so VTs can pick different animations:

```jsx
startTransition(() => {
  addTransitionType('nav-forward');
  router.push('/detail/1');
});
```

Map types to CSS classes:

```jsx
<ViewTransition
  enter={{ 'nav-forward': 'slide-from-right', 'nav-back': 'slide-from-left', default: 'none' }}
  exit={{ 'nav-forward': 'slide-to-left', 'nav-back': 'slide-to-right', default: 'none' }}
  default="none"
>
  <Page />
</ViewTransition>
```

**TypeScript:** `ViewTransitionClassPerType` requires a `default` key.

### Types and Suspense

Types are available during navigation but **not** during subsequent Suspense reveals (separate transitions, no type). Use type maps for page-level enter/exit; use simple string props for Suspense reveals.

---

## Shared Element Transitions

Same `name` on two VTs — one unmounting, one mounting — creates a shared element morph:

```jsx
<ViewTransition name="hero-image">
  <img src="/thumb.jpg" onClick={() => startTransition(() => onSelect())} />
</ViewTransition>

// Other view — same name
<ViewTransition name="hero-image">
  <img src="/full.jpg" />
</ViewTransition>
```

- Only one VT with a given `name` can be mounted at a time — use unique names.
- `share` takes precedence over `enter`/`exit`. Think through each navigation path: when no pair forms, `enter`/`exit` fires instead. Consider whether the element needs a fallback animation for those paths.
- Never use fade-out exit on pages with shared morphs — use directional slide.

---

## Common Patterns

### Enter/Exit

```jsx
{show && (
  <ViewTransition enter="fade-in" exit="fade-out"><Panel /></ViewTransition>
)}
```

### List Reorder

```jsx
{items.map(item => (
  <ViewTransition key={item.id}><ItemCard item={item} /></ViewTransition>
))}
```

Trigger inside `startTransition`. Avoid wrapper `<div>`s between list and VT.

### Force Re-Enter with `key`

```jsx
<ViewTransition key={searchParams.toString()} enter="slide-up" default="none">
  <ResultsGrid />
</ViewTransition>
```

**Caution:** Wrapping `<Suspense>` with key remounts the boundary and refetches.

### Suspense Fallback to Content

Simple cross-fade:
```jsx
<ViewTransition>
  <Suspense fallback={<Skeleton />}><Content /></Suspense>
</ViewTransition>
```

Directional reveal:
```jsx
<Suspense fallback={<ViewTransition exit="slide-down"><Skeleton /></ViewTransition>}>
  <ViewTransition enter="slide-up" default="none"><Content /></ViewTransition>
</Suspense>
```

---

## How Multiple VTs Interact

Every VT matching the trigger fires simultaneously in a single `document.startViewTransition`. VTs in **different** transitions don't compete.

### Use `default="none"` Liberally

Without it, every VT fires the browser cross-fade on **every** transition. Always use `default="none"` and explicitly enable only desired triggers.

### Two Patterns Coexist

**Pattern A — Directional slides:** Type-keyed VT on each page, fires during navigation.
**Pattern B — Suspense reveals:** Simple string props, fires when data loads (no type).

They coexist because they fire at different moments. `default="none"` on both prevents cross-interference. Always pair `enter` with `exit`. Place directional VTs in page components, not layouts.

---

## Next.js Integration

`<ViewTransition>` works out of the box for `startTransition`/`Suspense` updates. To also animate `<Link>` navigations:

```js
experimental: { viewTransition: true }
```

This wraps every `<Link>` navigation in `document.startViewTransition`. Use `default="none"` to prevent competing animations.

`next/link` supports a native `transitionTypes` prop:
```tsx
<Link href="/products/1" transitionTypes={['nav-forward']}>View</Link>
```

---

## Accessibility

Always add reduced motion CSS to your global stylesheet:

```css
@media (prefers-reduced-motion: reduce) {
  ::view-transition-old(*),
  ::view-transition-new(*),
  ::view-transition-group(*) {
    animation-duration: 0s !important;
    animation-delay: 0s !important;
  }
}
```

---

# Implementation Workflow

**Follow these steps in order.** Start with the audit — do not skip it. Copy the CSS recipes from the CSS Recipes section below — do not write your own animation CSS.

## Step 1: Audit the App

Before writing any code, scan the codebase thoroughly. Search for:

- **Every `<Link>` and `router.push`** — open every file that contains one
- **Every `<Suspense>` boundary** — check what its fallback renders
- **Every page/route component** — each needs a VT placement decision
- **Persistent elements** (headers, navbars, sidebars) — need `viewTransitionName` isolation
- **Shared visual elements** on both source and target views
- **Skeleton-to-content control pairs** — if a fallback renders a control that also exists in the real content, both need a matching `viewTransitionName`

Then classify every navigation and produce a navigation map:

```
| Route           | Navigates to         | Direction    | VT pattern            |
|-----------------|----------------------|--------------|-----------------------|
| /               | /detail/[id]         | forward      | directional slide     |
| /detail/[id]    | /                    | back         | directional slide     |
| /detail/[id]    | /detail/[other]      | lateral      | key+share crossfade   |
| /tab/[a]        | /tab/[b]             | lateral      | key+share crossfade   |
| (Suspense)      | (content loads)      | —            | slide-up reveal       |
```

For each shared element (`name` prop), note where a pair forms and where it doesn't — this determines whether you need `enter`/`exit` as a fallback alongside `share`.

## Step 2: Add CSS Recipes

Copy the **complete** CSS recipe set from the CSS Animation Recipes section below into your global stylesheet. Don't write your own — the recipes handle staggered timing, motion blur, and reduced motion.

## Step 3: Isolate Persistent Elements

```jsx
<header style={{ viewTransitionName: "site-header" }}>...</header>
```

```css
::view-transition-group(site-header) {
  animation: none;
  z-index: 100;
}
```

For `backdrop-blur`/`backdrop-filter`, use the backdrop-blur workaround instead.

## Step 4: Add Directional Page Transitions

```jsx
startTransition(() => {
  addTransitionType('nav-forward');
  router.push('/detail/1');
});
```

Wrap each **page component** (not layout) in a type-keyed VT:

```jsx
<ViewTransition
  enter={{ "nav-forward": "nav-forward", "nav-back": "nav-back", default: "none" }}
  exit={{ "nav-forward": "nav-forward", "nav-back": "nav-back", default: "none" }}
  default="none"
>
  <div>...page content...</div>
</ViewTransition>
```

**Rules:** Always pair `enter` with `exit`. Always include `default: "none"`. Place in page components, not layouts. Only use directional slides for hierarchical navigation.

## Step 5: Add Suspense Reveals

```jsx
<Suspense fallback={<ViewTransition exit="slide-down"><Skeleton /></ViewTransition>}>
  <ViewTransition enter="slide-up" default="none"><AsyncContent /></ViewTransition>
</Suspense>
```

Use `default="none"` on content VT. Use simple string props (not type maps) — Suspense resolves have no type.

## Step 6: Add Shared Element Transitions

```jsx
// Source view
<ViewTransition name={`photo-${photo.id}`} share="morph" default="none">
  <Image src={photo.src} ... />
</ViewTransition>

// Target view — same name
<ViewTransition name={`photo-${photo.id}`} share="morph">
  <Image src={photo.src} ... />
</ViewTransition>
```

**Rules:** Names must be globally unique. Add `default="none"` on list-side shared elements. Never use fade-out exit with shared morphs.

## Step 7: Verify Each Navigation Path

Walk through every row in the navigation map from Step 1:

- Does the VT mount/unmount, or stay mounted (same-route)?
- For named VTs: does a shared pair form? If not, does `enter`/`exit` provide a fallback?
- Does `default="none"` block an animation you actually want?
- Do persistent elements stay static?
- Do Suspense reveals animate independently from directional navigations?

---

## Common Mistakes

- **Bare VT without `default="none"`** — fires cross-fade on every transition
- **Directional VT in a layout** — layouts persist, enter/exit won't fire on route changes
- **Fade-out exit with shared morphs** — conflicts with morph, use directional slide
- **Writing custom animation CSS** — use the recipes
- **Missing `default: "none"` in type-keyed objects** — TypeScript requires it, fallback is `"auto"`
- **Type maps on Suspense reveals** — Suspense resolves have no type, use string props
- **Raw `viewTransitionName` CSS to trigger animations** — React only starts view transitions when `<ViewTransition>` components are in the tree. Bare `viewTransitionName` is for isolating elements, not triggering animations.
- **`update` trigger for same-route navigations** — nested VTs steal the mutation from the parent. Use `key` + `name` + `share` instead.

For Next.js-specific steps, see the Next.js section below.

---

# Patterns and Guidelines

## Searchable Grid with `useDeferredValue`

```tsx
'use client';

import { useDeferredValue, useState, ViewTransition, Suspense } from 'react';

export default function SearchableGrid({ itemsPromise }) {
  const [search, setSearch] = useState('');
  const deferredSearch = useDeferredValue(search);

  return (
    <>
      <input value={search} onChange={(e) => setSearch(e.currentTarget.value)} />
      <ViewTransition>
        <Suspense fallback={<GridSkeleton />}>
          <ItemGrid itemsPromise={itemsPromise} search={deferredSearch} />
        </Suspense>
      </ViewTransition>
    </>
  );
}
```

Per-item named VTs in deferred lists trigger cross-fades on every keystroke. Fix with `default="none"`.

## Card Expand/Collapse with `startTransition`

```tsx
'use client';

import { useState, useRef, startTransition, ViewTransition } from 'react';

export default function ItemGrid({ items }) {
  const [expandedId, setExpandedId] = useState(null);
  const scrollRef = useRef(0);

  return expandedId ? (
    <ViewTransition enter="slide-in" name={`item-${expandedId}`}>
      <ItemDetail
        item={items.find(i => i.id === expandedId)}
        onClose={() => {
          startTransition(() => {
            setExpandedId(null);
            setTimeout(() => window.scrollTo({ behavior: 'smooth', top: scrollRef.current }), 100);
          });
        }}
      />
    </ViewTransition>
  ) : (
    <div className="grid grid-cols-3 gap-4">
      {items.map(item => (
        <ViewTransition key={item.id} name={`item-${item.id}`}>
          <ItemCard
            item={item}
            onSelect={() => {
              scrollRef.current = window.scrollY;
              startTransition(() => setExpandedId(item.id));
            }}
          />
        </ViewTransition>
      ))}
    </div>
  );
}
```

## Cross-Fade Without Remount

Omit `key` to trigger update (cross-fade) instead of exit + enter. Avoids Suspense remount:

```jsx
<ViewTransition><TabPanel tab={activeTab} /></ViewTransition>
```

## Isolate Elements from Parent Animations

Persistent elements get captured in page's transition snapshot. Fix with `viewTransitionName`:

```jsx
<nav style={{ viewTransitionName: "persistent-nav" }}>{/* ... */}</nav>
```

```css
::view-transition-group(persistent-nav) { animation: none; z-index: 100; }
```

Same for floating elements (popovers, tooltips). Global fix: `::view-transition-group(*) { z-index: 100; }`

## Shared Controls Between Skeleton and Content

Give matching controls the same `viewTransitionName`. Don't put manual `viewTransitionName` on root DOM node inside `<ViewTransition>`.

## Reusable Animated Collapse

```jsx
function AnimatedCollapse({ open, children }) {
  if (!open) return null;
  return <ViewTransition enter="expand-in" exit="collapse-out">{children}</ViewTransition>;
}
```

## Preserve State with Activity

```jsx
<Activity mode={isVisible ? 'visible' : 'hidden'}>
  <ViewTransition enter="slide-in" exit="slide-out"><Sidebar /></ViewTransition>
</Activity>
```

## Exclude Elements with `useOptimistic`

`useOptimistic` values update before snapshot, excluding them from animation. Use for controls; use committed state for animated content.

---

## View Transition Events

Imperative control via `onEnter`, `onExit`, `onUpdate`, `onShare`. Always return cleanup. `onShare` takes precedence.

```jsx
<ViewTransition
  onEnter={(instance, types) => {
    const anim = instance.new.animate(
      [{ transform: 'scale(0.8)', opacity: 0 }, { transform: 'scale(1)', opacity: 1 }],
      { duration: 300, easing: 'ease-out' }
    );
    return () => anim.cancel();
  }}
>
  <Component />
</ViewTransition>
```

`instance`: `.old`, `.new`, `.group`, `.imagePair`, `.name`

---

## Animation Timing

| Interaction | Duration |
|------------|----------|
| Direct toggle | 100–200ms |
| Route transition | 150–250ms |
| Suspense reveal | 200–400ms |
| Shared element morph | 300–500ms |

---

## Troubleshooting

**VT not activating:** Ensure VT comes before any DOM node. Ensure `startTransition`.

**"Two VTs with same name":** Names must be globally unique. Use IDs.

**Back button skips animation:** Use Navigation API instead of legacy `popstate`.

**Only updates animate:** Without `<Suspense>`, React treats swaps as updates. Conditionally render the VT itself, or wrap in `<Suspense>`.

**Competing double animations:** Use `default="none"` on layout-level VTs.

**TS error "Property 'default' is missing":** Type-keyed objects require a `default` key.

**Backdrop-blur flickers:** `::view-transition-old(name) { display: none }` + `::view-transition-new(name) { animation: none }`.

**`border-radius` lost:** Apply `border-radius` directly to captured element.

**Batching:** Multiple updates during animation are batched (A→B→C→D becomes B→D).

---

# CSS Animation Recipes

Ready-to-use CSS for `<ViewTransition>` props. Copy into global stylesheet.

## Timing Variables

```css
:root {
  --duration-exit: 150ms;
  --duration-enter: 210ms;
  --duration-move: 400ms;
}
```

### Shared Keyframes

```css
@keyframes fade {
  from { filter: blur(3px); opacity: 0; }
  to { filter: blur(0); opacity: 1; }
}

@keyframes slide {
  from { translate: var(--slide-offset); }
  to { translate: 0; }
}

@keyframes slide-y {
  from { transform: translateY(var(--slide-y-offset, 10px)); }
  to { transform: translateY(0); }
}
```

## Fade

```css
::view-transition-old(.fade-out) {
  animation: var(--duration-exit) ease-in fade reverse;
}
::view-transition-new(.fade-in) {
  animation: var(--duration-enter) ease-out var(--duration-exit) both fade;
}
```

## Slide (Vertical)

```css
::view-transition-old(.slide-down) {
  animation:
    var(--duration-exit) ease-out both fade reverse,
    var(--duration-exit) ease-out both slide-y reverse;
}
::view-transition-new(.slide-up) {
  animation:
    var(--duration-enter) ease-in var(--duration-exit) both fade,
    var(--duration-move) ease-in both slide-y;
}
```

## Directional Navigation

### Single-Class Approach

```css
::view-transition-old(.nav-forward) {
  --slide-offset: -60px;
  animation:
    var(--duration-exit) ease-in both fade reverse,
    var(--duration-move) ease-in-out both slide reverse;
}
::view-transition-new(.nav-forward) {
  --slide-offset: 60px;
  animation:
    var(--duration-enter) ease-out var(--duration-exit) both fade,
    var(--duration-move) ease-in-out both slide;
}

::view-transition-old(.nav-back) {
  --slide-offset: 60px;
  animation:
    var(--duration-exit) ease-in both fade reverse,
    var(--duration-move) ease-in-out both slide reverse;
}
::view-transition-new(.nav-back) {
  --slide-offset: -60px;
  animation:
    var(--duration-enter) ease-out var(--duration-exit) both fade,
    var(--duration-move) ease-in-out both slide;
}
```

### Separate Enter/Exit Classes

```css
::view-transition-new(.slide-from-right) {
  --slide-offset: 60px;
  animation:
    var(--duration-enter) ease-out var(--duration-exit) both fade,
    var(--duration-move) ease-in-out both slide;
}
::view-transition-old(.slide-to-left) {
  --slide-offset: -60px;
  animation:
    var(--duration-exit) ease-in both fade reverse,
    var(--duration-move) ease-in-out both slide reverse;
}

::view-transition-new(.slide-from-left) {
  --slide-offset: -60px;
  animation:
    var(--duration-enter) ease-out var(--duration-exit) both fade,
    var(--duration-move) ease-in-out both slide;
}
::view-transition-old(.slide-to-right) {
  --slide-offset: 60px;
  animation:
    var(--duration-exit) ease-in both fade reverse,
    var(--duration-move) ease-in-out both slide reverse;
}
```

## Shared Element Morph

```css
::view-transition-group(.morph) {
  animation-duration: var(--duration-move);
}
::view-transition-image-pair(.morph) {
  animation-name: via-blur;
}
@keyframes via-blur {
  30% { filter: blur(3px); }
}
```

## Scale

```css
::view-transition-old(.scale-out) {
  animation: var(--duration-exit) ease-in scale-down;
}
::view-transition-new(.scale-in) {
  animation: var(--duration-enter) ease-out var(--duration-exit) both scale-up;
}
@keyframes scale-down {
  from { transform: scale(1); opacity: 1; }
  to { transform: scale(0.85); opacity: 0; }
}
@keyframes scale-up {
  from { transform: scale(0.85); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
```

## Persistent Element Isolation

```css
::view-transition-group(persistent-nav) {
  animation: none;
  z-index: 100;
}
```

### Backdrop-Blur Workaround

```css
::view-transition-old(persistent-nav) { display: none; }
::view-transition-new(persistent-nav) { animation: none; }
```

## Reduced Motion

```css
@media (prefers-reduced-motion: reduce) {
  ::view-transition-old(*),
  ::view-transition-new(*),
  ::view-transition-group(*) {
    animation-duration: 0s !important;
    animation-delay: 0s !important;
  }
}
```

---

# View Transitions in Next.js

## Setup

```js
// next.config.js
experimental: { viewTransition: true }
```

Wraps every `<Link>` navigation in `document.startViewTransition`. Use `default="none"` to prevent competing animations. Requires `react@canary`.

## Next.js Implementation Additions

**After Step 2:** Enable the experimental flag.

**Step 4:** Use `transitionTypes` on `<Link>`:
```tsx
<Link href="/photo/1" transitionTypes={["nav-forward"]}>View</Link>
<Link href="/" transitionTypes={["nav-back"]}>Back</Link>
```

**After Step 6:** For same-route dynamic segments, use `key` + `name` + `share` pattern.

## Layout-Level ViewTransition

Don't add layout-level VT if pages have their own VTs. Layouts persist — enter/exit only fire on initial mount. Use `default="none"` on layout VTs.

## The `transitionTypes` Prop

Native prop on `next/link`, works in Server Components:
```tsx
<Link href="/products/1" transitionTypes={['nav-forward']}>View</Link>
```

Reserve `startTransition` + `addTransitionType` for non-link interactions.

## Directional Navigation

Place type-keyed VTs in **page components** (not layouts):

```tsx
<ViewTransition
  default="none"
  enter={{ 'nav-forward': 'slide-from-right', 'nav-back': 'slide-from-left', default: 'none' }}
  exit={{ 'nav-forward': 'slide-to-left', 'nav-back': 'slide-to-right', default: 'none' }}
>
  <PageContent />
</ViewTransition>
```

### Two-Layer Pattern

```tsx
<ViewTransition
  enter={{ "nav-forward": "slide-from-right", default: "none" }}
  exit={{ "nav-forward": "slide-to-left", default: "none" }}
  default="none"
>
  <div>
    <Suspense fallback={<ViewTransition exit="slide-down"><Skeleton /></ViewTransition>}>
      <ViewTransition enter="slide-up" default="none"><Content /></ViewTransition>
    </Suspense>
  </div>
</ViewTransition>
```

## Shared Elements Across Routes

```tsx
// List page
<Link href={`/products/${product.id}`} transitionTypes={['nav-forward']}>
  <ViewTransition name={`product-${product.id}`}>
    <Image src={product.image} alt={product.name} width={400} height={300} />
  </ViewTransition>
</Link>

// Detail page — same name
<ViewTransition name={`product-${product.id}`}>
  <Image src={product.image} alt={product.name} width={800} height={600} />
</ViewTransition>
```

## Same-Route Dynamic Segment Transitions

Page stays mounted on dynamic segment change — enter/exit never fire. Use `key` + `name` + `share`:

```tsx
<Suspense fallback={<Skeleton />}>
  <ViewTransition key={slug} name={`collection-${slug}`} share="auto" default="none">
    <Content slug={slug} />
  </ViewTransition>
</Suspense>
```

## Server Components

- `<ViewTransition>` works in Server and Client Components
- `<Link transitionTypes>` works in Server Components
- `addTransitionType` and programmatic nav require Client Components
