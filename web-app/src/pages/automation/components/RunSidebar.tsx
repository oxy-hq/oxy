/**
 * Right-side output panel for the automation run page.
 *
 * Mirrors the legacy `AutomationOutput` layout: PanelHeader with an X close
 * button, a toolbar row holding the run-history dropdown and the show-logs
 * toggle, then the live content rendered through the same `OutputLogs`
 * component the legacy UI uses.
 *
 * Step status is conveyed by the diagram (border colors) and by the
 * collapsible step heading inside each log item — matching the legacy UX.
 * "Show logs" toggles between full nested logs and results-only.
 */

import { Check, Copy, FoldVertical, Maximize2, Minimize2, UnfoldVertical } from "lucide-react";
import type React from "react";
import { useEffect, useMemo, useState } from "react";
import OutputLogs from "@/components/automation/output/Logs";
import { useCopyTimeout } from "@/components/automation/output/useCopyTimeout";
import { Checkbox } from "@/components/ui/checkbox";
import EmptyState from "@/components/ui/EmptyState";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type {
  AutomationRunStream,
  RunStepState
} from "@/hooks/api/agentic-automations/useAgenticAutomations";
import { useLogItems } from "@/hooks/api/agentic-automations/useLogItems";
import type { AutomationEvent } from "@/services/api/automations";
import type { LogItem } from "@/services/types/logs";

import { IterationGrid, liveIterationsToOutcomes } from "./IterationGrid";
import RunSelector from "./RunSelector";
import { Trace } from "./Trace";

type Props = {
  automationRef: string;
  runId?: string;
  onSelectRun: (runId: string) => void;

  events: AutomationEvent[];
  phase: AutomationRunStream["phase"];
  error?: string;

  showLogs: boolean;
  onShowLogsChange: (show: boolean) => void;

  onClose: () => void;

  /**
   * Full per-step state from `useAutomationRunStream`. Used by the
   * Iterations tab to look up the selected loop's live
   * `LiveIteration[]`. The Output/Trace tabs don't need this.
   */
  steps?: RunStepState[];
  /**
   * Step name of the loop the user clicked on (via
   * `LoopProgressBar`). `null` = nothing selected → tab is hidden.
   */
  selectedLoop?: string | null;
  /**
   * Called when the user clears the selection (closes the panel
   * or switches off the Iterations tab). Setting this back to
   * `null` is what hides the tab.
   */
  onSelectedLoopChange?: (next: string | null) => void;

  /**
   * When `true` the sidebar is taking over the full panel area
   * (mobile UX). Drives the icon shown by the fullscreen toggle.
   * Optional so embedded consumers that don't offer fullscreen can
   * skip both props.
   */
  isFullScreen?: boolean;
  /**
   * Click handler for the fullscreen toggle button. When omitted the
   * button is hidden — only the automation page's mobile layout wires
   * this up today.
   */
  onToggleFullScreen?: () => void;
};

export const RunSidebar: React.FC<Props> = ({
  automationRef,
  runId,
  onSelectRun,
  events,
  phase,
  error,
  showLogs,
  onShowLogsChange,
  onClose,
  steps,
  selectedLoop,
  onSelectedLoopChange,
  isFullScreen = false,
  onToggleFullScreen
}) => {
  const subtitle = error ? `${phaseLabel(phase)} — ${error}` : phaseLabel(phase);
  const isRunning = phase === "starting" || phase === "running";
  const logs = useLogItems(events);

  // Three tabs: Output (default), Trace, Iterations. Iterations
  // only appears when a loop is selected (via the in-node progress
  // bar's click) — the tab list dynamically includes/excludes it.
  type TabKey = "output" | "trace" | "iterations";
  const [tab, setTab] = useState<TabKey>("output");

  // Auto-switch to Iterations when the user clicks a loop's
  // progress bar, then auto-switch back when the loop is cleared.
  // Using an effect keeps the click handler simple (it just sets
  // selectedLoop) without coupling the page's click semantics to
  // sidebar tab state.
  useEffect(() => {
    if (selectedLoop) {
      setTab("iterations");
    } else if (tab === "iterations") {
      setTab("output");
    }
    // We intentionally don't depend on `tab` — that would
    // immediately switch back when the user manually picks a
    // different tab while a loop is still selected.
    // biome-ignore lint/correctness/useExhaustiveDependencies: see comment above
  }, [selectedLoop, tab]);

  // Live iterations for the selected loop, lifted into the
  // grid's snapshot shape. Empty record collapses to the empty
  // state inside IterationGrid.
  const liveIterations = useMemo(() => {
    if (!selectedLoop) return {};
    const step = steps?.find((s) => s.name === selectedLoop);
    if (!step?.iterations) return {};
    return liveIterationsToOutcomes(selectedLoop, step.iterations);
  }, [selectedLoop, steps]);

  // Match legacy ergonomics: expand-all / collapse-all + copy-all live in
  // the panel header. The bumps drive `OutputLogs`'s internal item-state.
  const [expandAll, setExpandAll] = useState(0);
  const [collapseAll, setCollapseAll] = useState(0);
  const [allExpanded, setAllExpanded] = useState(false);
  const { copied, handleCopy } = useCopyTimeout();

  const toggleAll = () => {
    if (allExpanded) {
      setCollapseAll((n) => n + 1);
    } else {
      setExpandAll((n) => n + 1);
    }
    setAllExpanded((prev) => !prev);
  };

  const copyAll = () => handleCopy(getAllContent(logs));

  // Header copy/expand actions only make sense on the Output tab.
  // Fullscreen toggle (when offered) is always relevant — keep it
  // mounted as long as the parent wired up `onToggleFullScreen`,
  // even when there are no logs to expand/copy.
  //
  // Icon convention matches the legacy `AutomationOutput`:
  //   FoldVertical / UnfoldVertical → expand-all rows
  //   Maximize2 / Minimize2         → fullscreen the whole panel
  const showExpandCopy = tab === "output" && logs.length > 0;
  const headerActions =
    showExpandCopy || onToggleFullScreen ? (
      <>
        {showExpandCopy && (
          <>
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              onClick={toggleAll}
              aria-label={allExpanded ? "Collapse all rows" : "Expand all rows"}
              tooltip={allExpanded ? "Collapse all rows" : "Expand all rows"}
            >
              {allExpanded ? (
                <FoldVertical className='h-4 w-4' />
              ) : (
                <UnfoldVertical className='h-4 w-4' />
              )}
            </Button>
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              onClick={copyAll}
              aria-label='Copy all output'
              tooltip='Copy all output'
            >
              {copied ? <Check className='h-4 w-4 text-success' /> : <Copy className='h-4 w-4' />}
            </Button>
          </>
        )}
        {onToggleFullScreen && (
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={onToggleFullScreen}
            aria-label={isFullScreen ? "Exit full screen" : "Full screen"}
            tooltip={isFullScreen ? "Exit full screen" : "Full screen"}
          >
            {isFullScreen ? <Minimize2 className='h-4 w-4' /> : <Maximize2 className='h-4 w-4' />}
          </Button>
        )}
      </>
    ) : undefined;

  const outputBody =
    logs.length === 0 ? (
      <PanelContent>
        <EmptyState
          className='mt-[150px] [&>img]:opacity-100'
          title='No logs yet'
          description='Run the automation to see the logs'
        />
      </PanelContent>
    ) : (
      <PanelContent scrollable={false} padding={false}>
        <OutputLogs
          isPending={isRunning}
          logs={logs}
          onlyShowResult={!showLogs}
          expandAll={expandAll}
          collapseAll={collapseAll}
        />
      </PanelContent>
    );

  return (
    <Panel>
      <PanelHeader title='Output' subtitle={subtitle} actions={headerActions} onClose={onClose} />
      <div className='flex shrink-0 items-center justify-between gap-3 border-b px-4 py-2'>
        <RunSelector automationRef={automationRef} runId={runId} onSelect={onSelectRun} />
        <div className='flex items-center gap-2'>
          <Checkbox
            id='show_automation_logs'
            checked={showLogs}
            onCheckedChange={(v) => onShowLogsChange(v === true)}
          />
          <Label htmlFor='show_automation_logs' className='text-xs'>
            Show logs
          </Label>
        </div>
      </div>

      <Tabs
        value={tab}
        onValueChange={(v) => {
          const next = v as TabKey;
          setTab(next);
          // When the user manually navigates away from
          // Iterations, drop the selected loop so the tab
          // disappears (and a future click on the same loop's
          // bar still triggers the effect that re-selects it).
          if (next !== "iterations" && selectedLoop) {
            onSelectedLoopChange?.(null);
          }
        }}
        className='flex min-h-0 flex-1 flex-col'
      >
        <TabsList className='mx-4 mt-2 self-start'>
          <TabsTrigger value='output'>Output</TabsTrigger>
          <TabsTrigger value='trace'>Trace</TabsTrigger>
          {selectedLoop && (
            <TabsTrigger value='iterations'>
              Iterations{" "}
              <span className='ml-1 truncate font-mono text-[10px] text-muted-foreground'>
                {selectedLoop}
              </span>
            </TabsTrigger>
          )}
        </TabsList>
        {/* `flex flex-col` turns the active TabsContent into a flex
            container so the inner `PanelContent` (logs body) /
            `Trace` (overflow-y-auto) can resolve their `flex-1` /
            height. Without it the inner body collapses to its
            content height and either overflows the panel or leaves
            empty space at the bottom. */}
        <TabsContent value='output' className='flex min-h-0 flex-1 flex-col'>
          {outputBody}
        </TabsContent>
        <TabsContent value='trace' className='flex min-h-0 flex-1 flex-col overflow-y-auto'>
          <Trace events={events} />
        </TabsContent>
        <TabsContent
          value='iterations'
          className='flex min-h-0 flex-1 flex-col overflow-y-auto p-4'
        >
          {selectedLoop && Object.keys(liveIterations).length > 0 ? (
            <IterationGrid steps={liveIterations} mode='view' />
          ) : (
            <EmptyState
              className='mt-[100px] [&>img]:opacity-100'
              title='No iterations yet'
              description='The loop will fan out once the run reaches this step.'
            />
          )}
        </TabsContent>
      </Tabs>
    </Panel>
  );
};

function phaseLabel(phase: AutomationRunStream["phase"]): string {
  switch (phase) {
    case "idle":
      return "Idle";
    case "starting":
      return "Starting…";
    case "running":
      return "Running";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
    default:
      return phase;
  }
}

function getAllContent(items: LogItem[]): string {
  let out = "";
  for (const item of items) {
    if (item.children && item.children.length > 0) {
      out += getAllContent(item.children);
    } else {
      out += `${item.content}\n\n`;
    }
  }
  return out.trim();
}
