import { GripVerticalIcon } from "lucide-react";
import type * as React from "react";
import { createContext, useContext } from "react";
import * as ResizablePrimitive from "react-resizable-panels";

import { cn } from "@/libs/shadcn/utils";

type Direction = "horizontal" | "vertical";

const ResizableDirectionContext = createContext<Direction>("horizontal");

const toPercent = (v: number | string | undefined): string | undefined =>
  typeof v === "number" ? `${v}%` : v;

type GroupBaseProps = Omit<React.ComponentProps<typeof ResizablePrimitive.Group>, "orientation">;

type ResizablePanelGroupProps = GroupBaseProps & {
  direction?: Direction;
  autoSaveId?: string;
};

function PersistedGroup({
  autoSaveId,
  ...props
}: GroupBaseProps & { orientation: Direction; autoSaveId: string }) {
  const { defaultLayout, onLayoutChanged } = ResizablePrimitive.useDefaultLayout({
    id: autoSaveId
  });
  return (
    <ResizablePrimitive.Group
      defaultLayout={defaultLayout}
      onLayoutChanged={onLayoutChanged}
      {...props}
    />
  );
}

function ResizablePanelGroup({
  direction = "horizontal",
  autoSaveId,
  className,
  ...props
}: ResizablePanelGroupProps) {
  const groupClassName = cn(
    "flex h-full w-full data-[panel-group-direction=vertical]:flex-col",
    className
  );
  const sharedProps = {
    "data-slot": "resizable-panel-group",
    "data-panel-group-direction": direction,
    orientation: direction,
    className: groupClassName,
    ...props
  };
  return (
    <ResizableDirectionContext.Provider value={direction}>
      {autoSaveId ? (
        <PersistedGroup autoSaveId={autoSaveId} {...sharedProps} />
      ) : (
        <ResizablePrimitive.Group {...sharedProps} />
      )}
    </ResizableDirectionContext.Provider>
  );
}

function ResizablePanel({
  defaultSize,
  minSize,
  maxSize,
  collapsedSize,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Panel>) {
  return (
    <ResizablePrimitive.Panel
      data-slot='resizable-panel'
      defaultSize={toPercent(defaultSize)}
      minSize={toPercent(minSize)}
      maxSize={toPercent(maxSize)}
      collapsedSize={toPercent(collapsedSize)}
      {...props}
    />
  );
}

function ResizableHandle({
  withHandle,
  className,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Separator> & {
  withHandle?: boolean;
}) {
  const direction = useContext(ResizableDirectionContext);
  return (
    <ResizablePrimitive.Separator
      data-slot='resizable-handle'
      data-panel-group-direction={direction}
      className={cn(
        "relative flex w-px items-center justify-center bg-border after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1 data-[panel-group-direction=vertical]:h-px data-[panel-group-direction=vertical]:w-full data-[panel-group-direction=vertical]:after:left-0 data-[panel-group-direction=vertical]:after:h-1 data-[panel-group-direction=vertical]:after:w-full data-[panel-group-direction=vertical]:after:translate-x-0 data-[panel-group-direction=vertical]:after:-translate-y-1/2 [&[data-panel-group-direction=vertical]>div]:rotate-90",
        className
      )}
      {...props}
    >
      {withHandle && (
        <div className='z-10 flex h-4 w-3 items-center justify-center rounded-xs border bg-border'>
          <GripVerticalIcon className='size-2.5' />
        </div>
      )}
    </ResizablePrimitive.Separator>
  );
}

export { ResizableHandle, ResizablePanel, ResizablePanelGroup };
