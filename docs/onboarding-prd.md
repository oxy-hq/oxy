# PRD: Local-First Mode + Project Manager + Onboarding

**Status:** Mostly done — a few UX gaps remain
**Last updated:** 2026-04-01

---

## What we built

Oxy no longer requires `--cloud`, a workspace, or a git repo to start. All projects live under a
well-known root (`~/.local/share/oxy/projects/` by default). The app guides new users through
creating their first project, then provides a project manager for switching between them.

```
oxy start / oxy serve       → multi-project mode; shows onboarding if no projects, project manager otherwise
oxy serve /path/to/project  → single-project mode; disables project management UI
```

---

## Current actual user flow

### First-time user

1. Server starts → `needs_onboarding: true` → all routes redirect to `/setup`
2. `/setup` shows three options: Demo project, GitHub import, Blank project (with name input)
3. After creation → post-onboarding tour (shows LLM key status per provider, "Enter App" button)
4. "Enter App" → full reload → `MainLayout` loads with the new project active

### Returning user

- `needs_onboarding: false` → loads `MainLayout` with the active project from the DB
- Project switcher in sidebar lists all projects; clicking switches + reloads data
- `/setup` always accessible — shows existing projects on the left, create-new on the right

### Deleting a project

- Trash icon on hover → confirmation dialog → deletes from disk + DB → stays on `/projects`

---

## Admin bootstrap

**Problem solved:** No admin existed in fresh installs (chicken-and-egg with member management).

- **First user** to log in (any auth provider) automatically gets `UserRole::Admin` in the DB
- `LOCAL_GUEST_EMAIL` (no-auth mode) always gets Admin
- **`OXY_ADMINS=email1,email2`** env var: any listed email is promoted to Admin on login — this is
  the recovery path when no admin exists and auth is already enabled
- `config.admins` **removed** — admins are instance-wide, not per-project

---

## Admin visibility

"Manage members" in the footer shows whenever `isAdmin = true`:

- No-auth mode: everyone is implicitly admin (`!auth_enabled`)
- Single-project mode (`oxy serve /path`): everyone is implicitly admin (`single_project === true`) — no multi-tenancy, no user management needed
- Auth mode (multi-project): shows only for users whose DB role is `Admin`

## Single-project mode

When started as `oxy serve /path/to/project`, `projects_root` is `None` on the server.
The `/auth/config` response includes `single_project: true`.

Consequences:

- Project management UI (switcher, `/projects` page) is hidden
- Admin permission checks are bypassed: all authenticated users have full access
  - Frontend: `isAdmin` is `true` regardless of DB role
  - Backend: `secrets_access_middleware` skips the role check

---

## Key design decisions

| Decision | Reason |
| --- | --- |
| `/setup` route (not inline render) | Stable, bookmarkable URL for onboarding |
| No `authConfig` invalidation on project delete | Prevented unintended redirect to `/setup` |
| `admins` removed from `config.yml` | Admins are instance-scoped, not project-scoped |
| `OXY_ADMINS` env var for admin grant | Bootstrap path when no admin exists |
| First registered user = Admin | Bootstrap path for fresh DB |
| GitHub clone runs in background | Large repos don't hit request timeout |
| Cloning projects shown as grayed-out | Prevents switching to incomplete project |
| `GITHUB_APP_INSTALLATION_ID` env var | Skip GitHub connect dialog for pre-configured installs |
| IDE routes project-scoped (`/projects/:id/ide/...`) | IDE links are bookmarkable |
| Flat routes for chat/threads/context-graph | Project context from Zustand store, not URL |

---

## Still outstanding

### Frontend

| Item | Notes |
| --- | --- |
| Full reload after onboarding | `window.location.replace("/")` after tour; should be `navigate()` + query invalidation |
| Empty state after deleting last project | Deleting all projects leaves stale `needs_onboarding: false`; on refocus it redirects to `/setup` unexpectedly |
| ProjectSwitcher icon | Plus icon is misleading; should be grid/list |

### Backend

| Item | Notes |
| --- | --- |
| `last_opened_at` updated on activate | Field exists; unclear if `POST /projects/{id}/activate` updates it |
| Readiness check includes managed secrets | `/onboarding/readiness` only checks env vars, not secrets stored via Settings → Secrets |
