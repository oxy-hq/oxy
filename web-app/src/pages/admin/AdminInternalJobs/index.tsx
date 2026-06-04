import { DeadLetterSection } from "./components/DeadLetterSection";
import { QueueStatsSection } from "./components/QueueStatsSection";
import { RecentFailuresSection } from "./components/RecentFailuresSection";
import { ScheduledJobsSection } from "./components/ScheduledJobsSection";
import { WorkerFleetSection } from "./components/WorkerFleetSection";

/**
 * Internal Jobs admin surface.
 *
 * **NOT** the customer-facing Coordinator/Orchestrator UI. This page is for
 * Oxy staff to answer ops questions:
 *   - "Is the worker fleet healthy?"
 *   - "Did the dead-letter queue grow overnight?"
 *   - "When did the reaper last run?"
 *
 * Individual customer agentic runs live elsewhere (thread view, coordinator
 * page); this surface is the system-meta view across all tenants.
 */
export default function AdminInternalJobsPage() {
  return (
    <div className='flex flex-col gap-6 p-6'>
      <QueueStatsSection />
      <WorkerFleetSection />
      <ScheduledJobsSection />
      <RecentFailuresSection />
      <DeadLetterSection />
    </div>
  );
}
