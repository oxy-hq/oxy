import { ExternalLink, PanelRightClose, PanelRightOpen, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/shadcn/sheet";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { Thread } from "@/pages/thread";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useThreadDrawer from "@/stores/useThreadDrawer";

const WIDTH_KEY = "oxy:thread-drawer-width";
const DEFAULT_WIDTH = 672; // the old sm:max-w-2xl
const MIN_WIDTH = 400;

const clampWidth = (w: number) =>
  Math.min(Math.max(w, MIN_WIDTH), Math.round(window.innerWidth * 0.8));

const loadWidth = () => {
  const stored = Number(localStorage.getItem(WIDTH_KEY));
  return clampWidth(Number.isFinite(stored) && stored > 0 ? stored : DEFAULT_WIDTH);
};

/** Below sm (640px) the drawer is full-width and not resizable. */
const useIsDesktop = () => {
  const [v, setV] = useState(() => window.matchMedia("(min-width: 640px)").matches);
  useEffect(() => {
    const m = window.matchMedia("(min-width: 640px)");
    const fn = (e: MediaQueryListEvent) => setV(e.matches);
    m.addEventListener("change", fn);
    return () => m.removeEventListener("change", fn);
  }, []);
  return v;
};

/**
 * Right-side drawer hosting a live thread — answers stream in place so
 * the user never leaves the page they asked from. "Open full view"
 * promotes to the routed thread page. The left edge is a drag handle;
 * width persists across sessions.
 */
export function ThreadDrawer() {
  const { threadId, collapsed, collapse, expand, close } = useThreadDrawer();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const location = useLocation();
  const isDesktop = useIsDesktop();
  const [width, setWidth] = useState(loadWidth);
  const [dragging, setDragging] = useState(false);
  const widthRef = useRef(width);
  widthRef.current = width;

  // Collapse (don't dismiss) when the route changes while open — mirrors
  // AskPanel. In-drawer navigations or browser Back would otherwise leave the
  // drawer mounted over the destination page; collapsing keeps the thread
  // reachable via the edge tab. `openFull` calls close() before navigate, so
  // threadId is already null here and no tab shows.
  const pathRef = useRef(location.pathname);
  useEffect(() => {
    if (location.pathname !== pathRef.current) {
      pathRef.current = location.pathname;
      collapse();
    }
  }, [location.pathname, collapse]);

  const onHandlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDragging(true);
  }, []);
  const onHandlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return;
      setWidth(clampWidth(window.innerWidth - e.clientX));
    },
    [dragging]
  );
  const onHandlePointerUp = useCallback(() => {
    setDragging(false);
    localStorage.setItem(WIDTH_KEY, String(widthRef.current));
  }, []);

  const openFull = () => {
    // project is typed non-null inside workspace-scoped routes, but guard
    // defensively to match the project?.id pattern used in useHqAlerts.
    if (!threadId || !project?.id) return;
    const to = ROUTES.ORG(orgSlug).WORKSPACE(project.id).THREAD(threadId);
    close();
    navigate(to);
  };

  // The minimized edge tab: shown when a thread is bound but collapsed, on the
  // same surfaces as the Ask pill — which hides only on thread detail and
  // onboarding (see AskPill), NOT on /ide. The pill is available in the IDE, so
  // a drawer can be opened there; the tab must be available there too, or
  // collapsing (or navigating to /ide, which auto-collapses above) strands the
  // thread with no way to re-expand it.
  const showTab =
    !!threadId &&
    collapsed &&
    !/\/threads\/[^/]+\/?$/.test(location.pathname) &&
    !location.pathname.includes("/onboarding");

  return (
    <>
      <Sheet open={!!threadId && !collapsed} onOpenChange={(o) => !o && collapse()}>
        <SheetContent
          side='right'
          className={`flex w-full flex-col gap-0 p-0 sm:max-w-none [&>button]:hidden ${dragging ? "!transition-none select-none" : ""}`}
          style={isDesktop ? { width } : undefined}
          aria-describedby={undefined}
          data-testid='thread-drawer'
        >
          {isDesktop && (
            // biome-ignore lint/a11y/useSemanticElements: focusable widget separator (resize handle), not a static divider — <hr> can't express it
            <div
              role='separator'
              aria-orientation='vertical'
              aria-label='Resize drawer'
              aria-valuenow={width}
              aria-valuemin={MIN_WIDTH}
              aria-valuemax={Math.round(window.innerWidth * 0.8)}
              tabIndex={0}
              data-testid='thread-drawer-resize'
              onPointerDown={onHandlePointerDown}
              onPointerMove={onHandlePointerMove}
              onPointerUp={onHandlePointerUp}
              onKeyDown={(e) => {
                if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
                e.preventDefault();
                const next = clampWidth(width + (e.key === "ArrowLeft" ? 24 : -24));
                setWidth(next);
                localStorage.setItem(WIDTH_KEY, String(next));
              }}
              className='absolute inset-y-0 left-0 z-10 w-1.5 cursor-col-resize hover:bg-primary/20 focus-visible:bg-primary/30 focus-visible:outline-none'
            />
          )}
          {/* The built-in SheetContent close X is hidden (via [&>button]:hidden
              on SheetContent) — an X reads as "close" but here it would only
              collapse. We expose explicit Collapse (→ edge tab) and Close
              (→ dismiss) controls so each action is unambiguous. */}
          <SheetHeader className='flex-row items-center gap-1 border-b py-3 pr-3 pl-4'>
            <SheetTitle className='flex-1 text-sm'>Thread</SheetTitle>
            <Button
              variant='ghost'
              size='sm'
              onClick={openFull}
              data-testid='thread-drawer-open-full'
              className='gap-1.5 text-muted-foreground hover:text-foreground'
            >
              <ExternalLink className='size-3.5' />
              Full view
            </Button>
            <Button
              variant='ghost'
              size='icon'
              onClick={collapse}
              aria-label='Collapse'
              data-testid='thread-drawer-collapse'
              tooltip={{ content: "Collapse", side: "bottom" }}
              className='size-8 text-muted-foreground hover:text-foreground'
            >
              <PanelRightClose className='size-4' />
            </Button>
            <Button
              variant='ghost'
              size='icon'
              onClick={close}
              aria-label='Close thread'
              data-testid='thread-drawer-close'
              tooltip={{ content: "Close", side: "bottom" }}
              className='size-8 text-muted-foreground hover:text-foreground'
            >
              <X className='size-4' />
            </Button>
          </SheetHeader>
          <div className='min-h-0 flex-1 overflow-auto'>
            {threadId && <Thread key={threadId} threadId={threadId} hideHeader />}
          </div>
        </SheetContent>
      </Sheet>
      {showTab && (
        <div
          data-testid='thread-drawer-tab'
          className='group fixed inset-y-0 right-0 z-40 flex w-10 flex-col items-center border-l bg-background shadow-lg transition-colors hover:bg-accent'
        >
          <Button
            variant='ghost'
            size='icon'
            onClick={close}
            aria-label='Dismiss thread'
            data-testid='thread-drawer-dismiss'
            tooltip={{ content: "Dismiss thread", side: "left" }}
            className='mt-2 size-6 shrink-0 text-muted-foreground/70 hover:bg-transparent hover:text-foreground'
          >
            <X className='size-3' />
          </Button>
          <Button
            variant='ghost'
            onClick={expand}
            aria-label='Reopen thread'
            data-testid='thread-drawer-reopen'
            className='w-full flex-1 flex-col gap-2 rounded-none text-muted-foreground hover:bg-transparent group-hover:text-foreground'
          >
            <PanelRightOpen className='size-5' />
            <span className='rotate-180 text-xs tracking-wide [writing-mode:vertical-rl]'>
              Thread
            </span>
          </Button>
        </div>
      )}
    </>
  );
}
