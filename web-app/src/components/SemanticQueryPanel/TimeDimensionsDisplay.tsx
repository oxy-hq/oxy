import { Clock } from "lucide-react";
import type { TimeDimension } from "@/types/artifact";
import CollapsibleSection from "./CollapsibleSection";

interface TimeDimensionsDisplayProps {
  timeDimensions: TimeDimension[];
}

const TimeDimensionsDisplay = ({ timeDimensions }: TimeDimensionsDisplayProps) => {
  if (timeDimensions.length === 0) return null;

  return (
    <CollapsibleSection title='Time dimensions' count={timeDimensions.length}>
      <div className='flex flex-wrap gap-1.5'>
        {timeDimensions.map((td, i) => (
          <span
            key={`timedim-${td.dimension}-${td.granularity ?? "none"}-${i}`}
            className='inline-flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-xs'
          >
            <Clock className='h-3 w-3 text-muted-foreground' />
            <span className='font-medium'>{td.dimension.split(".").pop()}</span>
            {td.granularity && <span className='text-muted-foreground'>by {td.granularity}</span>}
          </span>
        ))}
      </div>
    </CollapsibleSection>
  );
};

export default TimeDimensionsDisplay;
