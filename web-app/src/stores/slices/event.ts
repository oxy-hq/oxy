import type { StateCreator } from "zustand";
import type { BlockEvent, TaskContent } from "@/services/types";
import type { GroupSlice } from "./group";

export interface EventSlice {
  handleEvent: (event: BlockEvent) => void;
}

export const createEventSlice: StateCreator<
  EventSlice & GroupSlice,
  [["zustand/devtools", never]],
  [],
  EventSlice
> = (_set, get) => ({
  handleEvent: (event) => {
    switch (event.type) {
      case "workflow_started": {
        const blockId = `${event.workflow_id}::${event.run_id}`;
        get().addGroup(blockId, {
          id: blockId,
          type: "workflow",
          workflow_id: event.workflow_id,
          run_id: event.run_id,
          workflow_config: event.workflow_config
        });
        get().setGroupProcessing(blockId, true);
        break;
      }
      case "workflow_finished": {
        const blockId = `${event.workflow_id}::${event.run_id}`;
        get().removeGroup(blockId, event.error);
        get().setGroupProcessing(blockId, false);
        break;
      }
      case "artifact_started": {
        const { artifact_id, artifact_name, artifact_metadata, is_verified } = event;
        get().addGroup(artifact_id, {
          id: artifact_id,
          type: "artifact",
          artifact_id,
          artifact_name,
          artifact_metadata,
          is_verified
        });
        get().setGroupProcessing(artifact_id, true);
        break;
      }
      case "artifact_finished": {
        const { artifact_id } = event;
        get().removeGroup(artifact_id, event.error);
        get().setGroupProcessing(artifact_id, false);
        break;
      }
      case "agentic_started": {
        const { agent_id, run_id } = event;
        const blockId = `${agent_id}::${run_id}`;
        get().addGroup(blockId, {
          id: blockId,
          type: "agentic",
          agent_id,
          run_id
        });
        get().setGroupProcessing(blockId, true);
        break;
      }
      case "agentic_finished": {
        const { agent_id, run_id } = event;
        const blockId = `${agent_id}::${run_id}`;
        get().removeGroup(blockId, event.error);
        get().setGroupProcessing(blockId, false);
        break;
      }

      case "task_started": {
        const { task_id, task_name, task_metadata } = event;
        get().upsertBlockToStack(task_id, {
          type: "task",
          task_name,
          task_metadata
        });
        break;
      }
      case "task_metadata": {
        const { task_id, metadata } = event;
        get().upsertBlockToStack(task_id, {
          type: "task",
          task_metadata: metadata
        } as TaskContent);
        break;
      }
      case "task_finished": {
        const { task_id } = event;
        get().removeBlockStack(task_id, event.error);
        break;
      }

      case "step_started": {
        const { id, step_type, objective } = event;
        get().upsertBlockToStack(id, {
          type: "step",
          id,
          step_type,
          objective
        });
        break;
      }
      case "step_finished": {
        const { step_id, error } = event;
        get().removeBlockStack(step_id, error);
        break;
      }

      case "content_added": {
        const { content_id, item } = event;
        get().upsertBlockToStack(content_id, item);
        break;
      }
      case "content_done": {
        const { content_id, item } = event;
        get().upsertBlockToStack(content_id, item);
        get().removeBlockStack(content_id);
        break;
      }
    }
  }
});
