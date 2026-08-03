/** One headline number in the activity summary. */
export const ActivityStat = ({
  id,
  label,
  value
}: {
  /** Stable slug for the testid — survives copy edits to `label`. */
  id: string;
  label: string;
  value: string;
}) => (
  <div className='rounded-md border bg-card p-3' data-testid={`admin-app-activity-stat-${id}`}>
    <div className='text-muted-foreground text-xs uppercase tracking-wider'>{label}</div>
    <div className='mt-1 font-medium text-sm'>{value}</div>
  </div>
);
