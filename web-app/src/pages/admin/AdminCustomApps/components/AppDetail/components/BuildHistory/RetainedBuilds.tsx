import { ChevronRight } from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { BuildHistory } from "./index";

/**
 * Every retained build, the ledger a rollback chooses from, behind its own
 * disclosure: the release timeline above shows the window's moves, and this the
 * builds that never moved in it.
 */
export const RetainedBuilds = ({ appId }: { appId: string }) => (
  <Collapsible data-testid='admin-app-retained-builds'>
    <CollapsibleTrigger className='group flex items-center gap-1.5 text-left text-muted-foreground text-xs hover:text-foreground'>
      <ChevronRight className='size-3 shrink-0 transition-transform group-data-[state=open]:rotate-90' />
      All retained builds
    </CollapsibleTrigger>
    <CollapsibleContent className='pt-2'>
      <BuildHistory appId={appId} />
    </CollapsibleContent>
  </Collapsible>
);
