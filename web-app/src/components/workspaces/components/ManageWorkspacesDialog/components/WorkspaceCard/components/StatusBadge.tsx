import { AlertTriangle } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";

type Props = {
  isActive: boolean;
  isCloning: boolean;
  isErrored: boolean;
};

export function StatusBadge({ isActive, isCloning, isErrored }: Props) {
  if (isCloning) {
    return (
      <span className='flex items-center gap-1.5 rounded-full bg-warning/10 px-2.5 py-1 font-medium text-warning text-xs'>
        <Spinner className='size-2.5' />
        Cloning…
      </span>
    );
  }
  if (isErrored) {
    return (
      <span className='flex items-center gap-1.5 rounded-full bg-destructive/10 px-2.5 py-1 font-medium text-destructive text-xs'>
        <AlertTriangle className='size-2.5' />
        Not an Oxy project
      </span>
    );
  }
  if (isActive) {
    return (
      <span className='rounded-full bg-primary/10 px-2 py-0.5 font-medium text-primary text-xs'>
        Current
      </span>
    );
  }
  return null;
}
