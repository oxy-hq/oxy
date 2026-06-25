import { useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { WorldModelService } from "@/services/api/worldModel";
import type {
  WmEntityCount,
  WmInstanceDetail,
  WmMeasureBreakdown,
  WmMeasureBreakdownEvent,
  WmMeasureName
} from "@/types/worldModel";
import queryKeys from "./queryKey";

export function useWorldModel() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery({
    queryKey: queryKeys.worldModel.graph(projectId),
    queryFn: () => WorldModelService.getWorldModel(projectId),
    staleTime: 5 * 60 * 1000,
    retry: false,
    // Harden against a partial response (e.g. a workspace whose semantic layer
    // has no relationships, where `edges` comes back null/absent): the graph
    // iterates `entities`/`edges`, so default them to arrays rather than
    // crashing with "model.edges is not iterable".
    select: (data) => ({
      ...data,
      entities: data?.entities ?? [],
      edges: data?.edges ?? []
    })
  });
}

export function useWmInstances(entityId: string | null, search: string) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery({
    queryKey: queryKeys.worldModel.instances(projectId, entityId ?? "", search),
    queryFn: () => WorldModelService.getInstances(projectId, entityId!, search, 50),
    enabled: !!entityId,
    staleTime: 60 * 1000,
    retry: false
  });
}

export function useWmFilterCounts(entityId: string | null, keyValue: string | null) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [counts, setCounts] = useState<Record<string, WmEntityCount> | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  // Buffer totals until matched arrives so nodes never flash "0 / N".
  const bufferRef = useRef<Record<string, { matched?: number; total?: number }>>({});

  const reset = useCallback(() => {
    abortRef.current?.abort();
    bufferRef.current = {};
    setCounts(null);
    setIsLoading(false);
  }, []);

  useEffect(() => {
    if (!entityId || !keyValue) {
      reset();
      return;
    }

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    bufferRef.current = {};
    setCounts(null);
    setIsLoading(true);

    WorldModelService.streamFilterCounts(
      projectId,
      entityId,
      keyValue,
      (event) => {
        if (event.done) {
          setIsLoading(false);
          return;
        }
        const buf = bufferRef.current;
        const entry = buf[event.entity_name] ?? {};
        if (event.total !== undefined) entry.total = event.total;
        if (event.matched !== undefined) entry.matched = event.matched;
        buf[event.entity_name] = entry;
        // Only surface the count once matched is known — avoids the "0 / N" flash.
        if (entry.matched !== undefined && entry.total !== undefined) {
          setCounts((prev) => ({
            ...prev,
            [event.entity_name]: { matched: entry.matched!, total: entry.total! }
          }));
        }
      },
      () => setIsLoading(false),
      controller.signal
    );

    return () => {
      controller.abort();
    };
  }, [projectId, entityId, keyValue, reset]);

  return { counts, isLoading };
}

export function useWmInstanceDetail(entityId: string | null, keyValue: string | null) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [data, setData] = useState<WmInstanceDetail | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  // measure_names arrives before init (no DB round-trip), so we buffer it here and
  // apply it synchronously when init fires to seed the skeleton rows.
  const pendingMeasureNamesRef = useRef<WmMeasureName[] | null>(null);

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setData(null);
    setIsLoading(false);
    pendingMeasureNamesRef.current = null;
  }, []);

  useEffect(() => {
    if (!entityId || !keyValue) {
      reset();
      return;
    }

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    pendingMeasureNamesRef.current = null;
    setData(null);
    setIsLoading(true);

    WorldModelService.streamInstanceDetail(
      projectId,
      entityId,
      keyValue,
      (event) => {
        if (event.kind === "done") {
          setIsLoading(false);
          return;
        }
        if (event.kind === "measure_names") {
          // Fires before init (schema-only, no DB). Buffer and apply when init arrives.
          pendingMeasureNamesRef.current = event.measure_names;
        } else if (event.kind === "init") {
          // Seed computed_measures from buffered measure_names so skeletons show immediately.
          const skeletons = (pendingMeasureNamesRef.current ?? []).map((n) => ({
            name: n.name,
            measure_type: n.measure_type,
            value: null as null,
            fiber_count: 0,
            label: n.label
          }));
          setData({
            entity_id: event.entity_id,
            key_value: event.key_value,
            display: event.display,
            attributes: event.attributes,
            promotes_to: [],
            receives_from: [],
            computed_measures: skeletons
          });
        } else if (event.kind === "parent") {
          setData((prev) => prev && { ...prev, promotes_to: event.promotes_to });
        } else if (event.kind === "child") {
          setData(
            (prev) => prev && { ...prev, receives_from: [...prev.receives_from, event.child] }
          );
        } else if (event.kind === "measure") {
          // Replace skeleton entries by name with real values, preserving order.
          const arrived = new Map(event.computed_measures.map((m) => [m.name, m]));
          setData(
            (prev) =>
              prev && {
                ...prev,
                computed_measures: prev.computed_measures.map((m) => arrived.get(m.name) ?? m)
              }
          );
        }
      },
      () => setIsLoading(false),
      controller.signal
    );

    return () => {
      controller.abort();
    };
  }, [projectId, entityId, keyValue, reset]);

  return { data, isLoading, error: null };
}

/**
 * Pure reducer folding measure-breakdown SSE events into the assembled tree.
 * `init` seeds every node with a null value; `value` fills one node by id.
 */
export function applyBreakdownEvent(
  state: WmMeasureBreakdown | null,
  event: WmMeasureBreakdownEvent
): WmMeasureBreakdown {
  if (event.kind === "init") {
    return {
      root: event.root,
      edges: event.edges,
      nodes: event.nodes.map((n) => ({ ...n, value: null, unvalued_reason: null }))
    };
  }
  if (event.kind === "value" && state) {
    return {
      ...state,
      nodes: state.nodes.map((n) =>
        n.id === event.node_id
          ? { ...n, value: event.value, unvalued_reason: event.unvalued_reason }
          : n
      )
    };
  }
  return state ?? { root: "", nodes: [], edges: [] };
}

export function useWmMeasureBreakdown(
  entityId: string | null,
  keyValue: string | null,
  measureName: string | null
) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const [data, setData] = useState<WmMeasureBreakdown | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setData(null);
    setIsLoading(false);
  }, []);

  useEffect(() => {
    if (!entityId || !keyValue || !measureName) {
      reset();
      return;
    }

    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setData(null);
    setIsLoading(true);

    WorldModelService.streamMeasureBreakdown(
      projectId,
      entityId,
      keyValue,
      measureName,
      (event) => {
        if (event.kind === "done") {
          setIsLoading(false);
          return;
        }
        setData((prev) => applyBreakdownEvent(prev, event));
      },
      () => setIsLoading(false),
      controller.signal
    );

    return () => {
      controller.abort();
    };
  }, [projectId, entityId, keyValue, measureName, reset]);

  return { data, isLoading };
}
