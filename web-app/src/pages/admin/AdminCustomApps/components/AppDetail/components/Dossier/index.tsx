import { ChevronRight, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import type { CustomApp } from "@/types/apps";
import type { DockMode } from "../../dock";
import { usePersistentState } from "../../usePersistentState";
import { Activity } from "../Activity";
import { AppInfo } from "../AppInfo";
import { AppSettings } from "../AppSettings";
import { BuildHistory } from "../BuildHistory";
import { Functions } from "../Functions";
import { DockControls } from "./DockControls";

export { DockControls };

type SectionId = "status" | "builds" | "functions" | "activity" | "settings";

/**
 * Open by default: the two an operator opens the panel *for*. The rest answer
 * follow-up questions, so they start collapsed — five expanded sections is what
 * made this column a scroll marathon.
 */
const DEFAULT_OPEN: Record<SectionId, boolean> = {
  status: true,
  builds: true,
  functions: false,
  activity: false,
  settings: false
};

const SECTIONS_STORAGE_KEY = "admin-app-dossier-sections";

const reviveOpenState = (raw: unknown): Record<SectionId, boolean> | null => {
  if (typeof raw !== "object" || raw === null) return null;
  const stored = raw as Record<string, unknown>;
  // Merge over the defaults so a key added in a later build still appears.
  return Object.fromEntries(
    Object.entries(DEFAULT_OPEN).map(([id, fallback]) => [
      id,
      typeof stored[id] === "boolean" ? stored[id] : fallback
    ])
  ) as Record<SectionId, boolean>;
};

/**
 * The dossier's own title strip — dock switcher on the right, exactly where
 * DevTools puts it. Separate from `DetailToolbar` on purpose: that row belongs
 * to the *preview*, and it's already full.
 */
export const DossierHeader = ({
  dock,
  onDockChange,
  onClose
}: {
  dock: DockMode;
  onDockChange: (next: DockMode) => void;
  onClose?: () => void;
}) => (
  <div className='flex h-9 shrink-0 items-center gap-2 border-b bg-background px-2'>
    <span className='ml-1 min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground uppercase tracking-wider'>
      Details
    </span>
    <DockControls value={dock} onChange={onDockChange} />
    {onClose && (
      <Button
        variant='ghost'
        size='icon'
        className='size-7'
        onClick={onClose}
        aria-label='Hide details'
      >
        <X className='size-3.5' />
      </Button>
    )}
  </div>
);

/**
 * The stacked dossier sections, shared by every placement (side column, bottom
 * drawer, popped-out window, narrow-screen sheet) so they can't drift.
 *
 * Sections reflow by the *panel's* width, not the viewport's — a container
 * query, because the panel is resizable and can be 400px or 1400px wide at the
 * same viewport size. Docked right it reads as one column; docked bottom the
 * same content fans out to two or three, which is the point of that placement:
 * one screenful instead of five, without label/value pairs stranded at opposite
 * ends of a 1400px row.
 */
export const DossierBody = ({ app }: { app: CustomApp }) => {
  const [open, setOpen] = usePersistentState(SECTIONS_STORAGE_KEY, DEFAULT_OPEN, reviveOpenState);
  const toggle = (id: SectionId) => (next: boolean) => setOpen({ ...open, [id]: next });

  return (
    <div className='@container min-h-0 flex-1 overflow-auto'>
      <div className='grid @3xl:grid-cols-2 @5xl:grid-cols-3 grid-cols-1 items-start gap-x-8'>
        <DossierSection
          title='Status & manifest'
          open={open.status}
          onOpenChange={toggle("status")}
        >
          <AppInfo app={app} />
        </DossierSection>
        <DossierSection title='Build history' open={open.builds} onOpenChange={toggle("builds")}>
          <div className='p-4 pt-0'>
            <BuildHistory appId={app.id} />
          </div>
        </DossierSection>
        <DossierSection title='Functions' open={open.functions} onOpenChange={toggle("functions")}>
          <div className='p-4 pt-0'>
            <Functions appId={app.id} />
          </div>
        </DossierSection>
        <DossierSection title='Activity' open={open.activity} onOpenChange={toggle("activity")}>
          <Activity appId={app.id} />
        </DossierSection>
        <DossierSection title='Settings' open={open.settings} onOpenChange={toggle("settings")}>
          <AppSettings app={app} />
        </DossierSection>
      </div>
    </div>
  );
};

/**
 * A collapsible block in the dossier. The header doubles as the disclosure
 * control, so the section costs one 32px row when it's closed and nothing at
 * all in horizontal space.
 *
 * Also a container in its own right: a section's *cell* is a third of the panel
 * once the grid splits, so anything inside that reflows (Activity's stat tiles,
 * BuildHistory's channel cards) must measure this box, not the whole panel —
 * otherwise a wide panel tells a 360px cell to lay out four columns.
 */
const DossierSection = ({
  title,
  open,
  onOpenChange,
  children
}: {
  title: string;
  open: boolean;
  onOpenChange: (next: boolean) => void;
  children: React.ReactNode;
}) => (
  <Collapsible
    open={open}
    onOpenChange={onOpenChange}
    className='@container min-w-0 border-border/60 border-b'
  >
    <CollapsibleTrigger className='group flex w-full items-center gap-1.5 px-4 py-2 text-left transition-colors hover:bg-muted/40'>
      <ChevronRight className='size-3 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-90' />
      <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.16em]'>
        {title}
      </span>
    </CollapsibleTrigger>
    <CollapsibleContent>{children}</CollapsibleContent>
  </Collapsible>
);
