import { Check } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useListTemplates } from "@/hooks/api/customApps/useCustomApps";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  value: string;
  onChange: (id: string) => void;
};

/**
 * Three-card gallery for the Create flow's "Start from" picker.
 * Each card shows the template name and a one-line description.
 * The selected card gets a primary border and a check overlay.
 *
 * No screenshots ship today — when the first template wants one,
 * re-add `screenshot_url` to `Template`, the `GET .../templates/{id}/screenshot`
 * route, and an `<img>` branch here.
 *
 * Responsive: 3 cols at md+, stacks to 1 col at narrower widths.
 */
export const TemplatePicker = ({ value, onChange }: Props) => {
  const { data, isLoading, error } = useListTemplates();

  if (isLoading) {
    return (
      <div className='grid grid-cols-1 gap-2 md:grid-cols-3'>
        <Skeleton className='h-32' />
        <Skeleton className='h-32' />
        <Skeleton className='h-32' />
      </div>
    );
  }

  // `Array.isArray` is belt-and-suspenders — `listTemplates` already
  // rejects non-array responses, but a cache hit from before that
  // validator landed could still hand us an HTML string.
  if (error || !Array.isArray(data)) {
    const detail = error instanceof Error ? error.message : null;
    return (
      <div className='text-destructive text-xs'>
        <p>Couldn't load templates. The server may be offline.</p>
        {detail && <p className='mt-1 text-muted-foreground text-xs'>{detail}</p>}
      </div>
    );
  }

  return (
    <div className='grid grid-cols-1 gap-2 md:grid-cols-3'>
      {data.map((t) => (
        <button
          key={t.id}
          type='button'
          onClick={() => onChange(t.id)}
          aria-pressed={value === t.id}
          className={cn(
            "relative flex flex-col items-stretch overflow-hidden rounded-md border border-border bg-card text-left transition-colors",
            "hover:border-primary/40",
            value === t.id && "border-primary ring-1 ring-primary"
          )}
        >
          <div className='aspect-[3/2] w-full bg-muted' />
          <div className='flex flex-col gap-1 p-3'>
            <span className='font-medium text-xs'>{t.name}</span>
            <span className='text-muted-foreground text-xs leading-snug'>{t.description}</span>
          </div>
          {value === t.id && (
            <span className='absolute top-2 right-2 flex size-5 items-center justify-center rounded-full bg-primary text-primary-foreground'>
              <Check className='size-3' />
            </span>
          )}
        </button>
      ))}
    </div>
  );
};
