import type { ReactFlowInstance, FitViewOptions } from "@xyflow/react";
import type { Viewport } from "../hooks/usePersistedViewport";

type InstanceWithViewport = {
  setViewport?: (v: Viewport) => void;
  setTransform?: (t: Viewport) => void;
  fitView?: (opts?: FitViewOptions) => void;
};

export const restoreOrFit = (
  instance: ReactFlowInstance | null,
  saved: Viewport | null,
  fitViewOptions?: FitViewOptions,
) => {
  if (!instance) return;

  const inst = instance as unknown as InstanceWithViewport;

  if (saved) {
    if (typeof inst.setViewport === "function") {
      inst.setViewport(saved);
      return;
    }
    if (typeof inst.setTransform === "function") {
      inst.setTransform({ x: saved.x, y: saved.y, zoom: saved.zoom });
      return;
    }
  } else {
    // If the instance can't set a saved viewport directly, skip fallback fit.
    inst.fitView?.(fitViewOptions);
  }
};
