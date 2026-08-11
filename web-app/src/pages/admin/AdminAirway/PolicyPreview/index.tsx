import type { UseQueryResult } from "@tanstack/react-query";
import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { AirwayPolicyPreviewResponse } from "@/services/api/airwayConfig";
import { PreviewResults } from "./components/PreviewResults";

interface PolicyPreviewProps {
  sourceKind: string;
  open: boolean;
  onRequestPreview: () => void;
  onHide: () => void;
  preview: UseQueryResult<AirwayPolicyPreviewResponse>;
}

/**
 * The preview disclosure for one source-kind card. Closed by default — the
 * underlying `usePolicyPreview` query is lazy, so nothing fires until the
 * operator explicitly asks. `SourceKindCard` owns `open`/the query itself
 * (via `preview`) so it can gate the Save confirm on the same data this
 * renders; this component is presentation-only.
 */
export function PolicyPreview({
  sourceKind,
  open,
  onRequestPreview,
  onHide,
  preview
}: PolicyPreviewProps) {
  if (!open) {
    return (
      <Button
        type='button'
        variant='outline'
        size='sm'
        onClick={onRequestPreview}
        data-testid={`admin-airway-preview-trigger-${sourceKind}`}
      >
        Preview this policy
      </Button>
    );
  }

  return (
    <Collapsible
      open
      onOpenChange={(next) => {
        if (!next) onHide();
      }}
      className='rounded-lg border'
    >
      <CollapsibleTrigger
        className='group flex w-full items-center justify-between px-3 py-2 text-left hover:bg-muted/40'
        data-testid={`admin-airway-preview-toggle-${sourceKind}`}
      >
        <span className='font-medium text-xs'>Preview</span>
        <ChevronDown className='size-3.5 text-muted-foreground transition-transform group-data-[state=open]:rotate-180' />
      </CollapsibleTrigger>
      <CollapsibleContent className='space-y-3 border-t px-3 py-3'>
        {preview.isPending ? (
          <div className='flex items-center justify-center gap-2 py-6 text-muted-foreground text-xs'>
            <Spinner className='size-3.5' /> Scanning compiled pipelines…
          </div>
        ) : preview.isError ? (
          <p
            className='text-destructive text-xs'
            data-testid={`admin-airway-preview-error-${sourceKind}`}
          >
            Failed to load preview.
          </p>
        ) : preview.data ? (
          <PreviewResults data={preview.data} sourceKind={sourceKind} />
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  );
}
