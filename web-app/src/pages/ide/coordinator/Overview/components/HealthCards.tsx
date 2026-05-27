import { Activity, CalendarClock, CheckCircle2, Loader2, XCircle } from "lucide-react";
import type React from "react";
import { MetricCard } from "../../components/MetricCard";
import type { OverviewMetrics } from "../useOverviewModel";

/** The five-card health strip — "is everything okay right now?" at a glance. */
export const HealthCards: React.FC<{ metrics: OverviewMetrics }> = ({ metrics }) => {
  const rate = metrics.successRate;
  const rateTone =
    rate === null ? "default" : rate >= 95 ? "success" : rate >= 80 ? "warning" : "destructive";

  return (
    <div className='grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5'>
      <MetricCard label='Active jobs' value={metrics.activeJobs} icon={CalendarClock} />
      <MetricCard label='Runs' value={metrics.runsInWindow} icon={Activity} />
      <MetricCard
        label='Success rate'
        value={rate === null ? "—" : `${rate}%`}
        icon={CheckCircle2}
        tone={rateTone}
      />
      <MetricCard
        label='Failed'
        value={metrics.failed}
        icon={XCircle}
        tone={metrics.failed > 0 ? "destructive" : "default"}
      />
      <MetricCard
        label='Running now'
        value={metrics.runningNow}
        icon={Loader2}
        tone={metrics.runningNow > 0 ? "primary" : "default"}
        live={metrics.runningNow > 0}
      />
    </div>
  );
};
