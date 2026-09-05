import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppAvailability } from "@/hooks/api/customApps/useCustomApps";
import { AvailabilityVerdict } from "./components/AvailabilityVerdict";
import { AvailabilityWindows } from "./components/AvailabilityWindows";

/**
 * Availability — the *serving* answer, as opposed to the deployment-integrity
 * ladder behind the status badge above it.
 *
 * A custom-app host answers 200 with the SPA shell for every path, so probing
 * an app from outside is green whatever the app is doing. Everything here is
 * derived instead from the requests real people made, which Oxy already
 * terminates: the success ratio over several windows, plus a multi-window
 * error-budget burn verdict.
 *
 * Three verdicts, and the third is not a flavour of healthy — see
 * `availabilityTone`.
 */
export const Availability = ({ orgSlug, appSlug }: { orgSlug: string; appSlug: string }) => {
  const { data, isLoading, error } = useAppAvailability(orgSlug, appSlug);

  if (isLoading) {
    return (
      <div className='space-y-3 p-4 pt-0'>
        <Skeleton className='h-14 w-full' />
        <Skeleton className='h-32 w-full' />
      </div>
    );
  }

  // 501 is the expected answer wherever `OXY_OBSERVABILITY_BACKEND` is unset —
  // every dev box, by default. A capability statement, not a fault, so it must
  // not read like one.
  const status = (error as { response?: { status?: number } } | null)?.response?.status;
  if (status === 501) {
    return (
      <p
        className='px-4 pb-4 text-muted-foreground text-xs leading-relaxed'
        data-testid='admin-app-availability-unconfigured'
      >
        Availability is not being measured — this deployment has no observability backend configured
        (<code className='font-mono'>OXY_OBSERVABILITY_BACKEND</code>). Nothing is wrong with the
        app; there is simply no data.
      </p>
    );
  }
  if (error || !data) {
    return (
      <p
        className='px-4 pb-4 text-muted-foreground text-xs leading-relaxed'
        data-testid='admin-app-availability-error'
      >
        Could not read availability. This says nothing about whether the app is up — only that the
        query failed.
      </p>
    );
  }

  return (
    <div className='space-y-3 p-4 pt-0' data-testid='admin-app-availability'>
      <AvailabilityVerdict data={data} />
      <AvailabilityWindows windows={data.windows} />
    </div>
  );
};
