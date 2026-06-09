import type { LucideProps } from "lucide-react";
import { Loader2Icon } from "lucide-react";

import { cn } from "@/libs/shadcn/utils";

function Spinner({ className, ...props }: LucideProps) {
  return (
    <Loader2Icon
      role='status'
      aria-label='Loading'
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}

export { Spinner };
