import { GitMerge } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  onClick: () => void;
}

export function ResolveButton({ onClick }: Props) {
  return (
    <Button
      size='sm'
      variant='outline'
      onClick={onClick}
      data-testid='ide-resolve-button'
      className='border-warning/40 text-status-warning-text hover:bg-status-warning-bg'
    >
      <GitMerge />
      Resolve conflicts
    </Button>
  );
}
