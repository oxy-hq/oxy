import { AlertTriangle } from "lucide-react";
import type { ProbeResponse } from "@/types/apps";
import { WarningText } from "./WarningText";

/** Last segment of an absolute path (trailing slash tolerant). */
const basenameOf = (path: string): string => {
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
};

/** Extract `<slug>` from `/customer-apps/<org>/<slug>/`. */
const slugFromBakedBasePath = (baked: string | null | undefined): string | null => {
  if (!baked) return null;
  const m = baked.match(/^\/customer-apps\/[^/]+\/([^/]+)\/?$/);
  return m?.[1] ?? null;
};

export type LinkPlan =
  | {
      ready: true;
      name: string;
      slug: string;
      orgId: string;
      orgSlug: string;
      orgName: string;
      projectId: string;
      branch: "main";
      /** Non-blocking informational notes (e.g. manifest hint differs from operator pick). */
      notes?: string[];
    }
  | {
      ready: false;
      warnings: string[];
      partial: {
        name?: string;
        slug?: string;
        orgSlug?: string;
        orgName?: string;
        projectId?: string;
      };
    };

/**
 * Derive the full link plan from the probe response, the org list, and the
 * operator's form picks.
 *
 * Identity is now operator-authoritative:
 *   - slug / name   → manifest hint, then fallback derivation (as before)
 *   - org + project → operator's form picks (formOrgId / formProjectId);
 *                     manifest hints are surfaced as informational notes only.
 *
 * Returns `{ ready: true, ... }` when every required field resolves,
 * or `{ ready: false, warnings, partial }` listing what's missing.
 *
 * This is the single authoritative computation for the link flow —
 * used for the summary display AND for `buildRequest`.
 */
export function summariseLinkPlan(
  probe: ProbeResponse | null,
  linkPath: string,
  orgs: { id: string; name: string; slug: string }[] | undefined,
  formOrgId: string | undefined,
  formProjectId: string | undefined
): LinkPlan {
  const warnings: string[] = [];
  const notes: string[] = [];

  if (!probe?.ok) {
    return {
      ready: false,
      warnings: ["Bundle probe must succeed before linking."],
      partial: {}
    };
  }

  const slug = probe.manifest_slug ?? slugFromBakedBasePath(probe.baked_base_path);
  const name = probe.manifest_name ?? basenameOf(linkPath);

  if (!slug) {
    warnings.push(
      "No slug in oxy-app.json and no baked base path detected — cannot derive a slug."
    );
  }

  if (!formOrgId) {
    warnings.push("Pick an organization.");
  }

  if (!formProjectId) {
    warnings.push("Pick a project.");
  }

  if (warnings.length > 0) {
    return {
      ready: false,
      warnings,
      partial: { name: name || undefined, slug: slug ?? undefined }
    };
  }

  // formOrgId and formProjectId are guaranteed non-empty here (warnings guard above).
  const org = orgs?.find((o) => o.id === formOrgId);
  if (!org) {
    return {
      ready: false,
      warnings: ["Selected organization not found."],
      partial: { name: name || undefined, slug: slug ?? undefined }
    };
  }

  // Surface a non-blocking note when the manifest's declared org differs from
  // the operator's pick — the operator wins, but this helps catch accidental mismatches.
  if (probe.manifest_org_slug && probe.manifest_org_slug !== org.slug) {
    notes.push(
      `Manifest declares orgSlug "${probe.manifest_org_slug}"; using your selection "${org.slug}".`
    );
  }

  // slug, formOrgId, and formProjectId are all guaranteed non-empty here:
  // any missing value was already caught by the warnings guard above.
  const resolvedSlug = slug as string;
  const resolvedOrgId = formOrgId as string;
  const resolvedProjectId = formProjectId as string;

  return {
    ready: true,
    name,
    slug: resolvedSlug,
    orgId: resolvedOrgId,
    orgSlug: org.slug,
    orgName: org.name,
    projectId: resolvedProjectId,
    branch: "main",
    notes
  };
}

type Props = {
  plan: LinkPlan;
};

/**
 * Read-only "Will be linked as:" summary rendered in the link flow.
 * Shows all resolved values (or partial values + per-missing warnings).
 * Live preview happens post-submit in AppDetail — running an iframe
 * inside the form was noticeably heavy on the dialog modal.
 */
export const LinkPlanSummary = ({ plan }: Props) => {
  const partial = plan.ready
    ? {
        name: plan.name,
        slug: plan.slug,
        orgSlug: plan.orgSlug,
        orgName: plan.orgName,
        projectId: plan.projectId
      }
    : plan.partial;

  return (
    <div className='flex flex-col gap-2 rounded-md border border-border bg-muted/40 p-3 text-xs'>
      <p className='font-medium'>Will be linked as:</p>
      <dl className='flex flex-col gap-1'>
        {partial.name && <SummaryField label='Name' value={partial.name} mono={false} />}
        {partial.slug && <SummaryField label='Slug' value={partial.slug} mono />}
        {partial.orgSlug && (
          <SummaryField
            label='Org'
            value={partial.orgName ? `${partial.orgName} (${partial.orgSlug})` : partial.orgSlug}
            mono={false}
          />
        )}
        {partial.projectId && (
          <SummaryField label='Project' value={`${partial.projectId.slice(0, 8)}…`} mono />
        )}
        {plan.ready && <SummaryField label='Branch' value='main' mono />}
      </dl>
      {!plan.ready &&
        plan.warnings.map((w) => (
          <div
            key={w}
            role='alert'
            className='flex items-start gap-2 rounded border border-destructive/40 bg-destructive/10 p-2 text-destructive text-xs'
          >
            <AlertTriangle className='mt-0.5 size-3.5 shrink-0' />
            <span>
              <WarningText text={w} />
            </span>
          </div>
        ))}
      {plan.ready && plan.notes && plan.notes.length > 0 && (
        <div className='flex flex-col gap-1'>
          {plan.notes.map((note) => (
            <p key={note} className='text-muted-foreground text-xs'>
              {note}
            </p>
          ))}
        </div>
      )}
    </div>
  );
};

const SummaryField = ({ label, value, mono }: { label: string; value: string; mono: boolean }) => (
  <div className='flex items-baseline gap-1.5'>
    <dt className='min-w-16 text-muted-foreground text-xs'>{label}</dt>
    <dd>
      {mono ? (
        <code className='break-all rounded bg-muted px-1 py-0.5 font-mono text-xs'>{value}</code>
      ) : (
        <span className='text-xs'>{value}</span>
      )}
    </dd>
  </div>
);
