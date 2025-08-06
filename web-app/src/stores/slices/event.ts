import { StateCreator } from "zustand";
import { BlockEvent, TaskContent } from "@/services/types";
import { GroupSlice } from "./group";

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
          workflow_config: event.workflow_config,
        });
        break;
      }
      case "workflow_finished": {
        const blockId = `${event.workflow_id}::${event.run_id}`;
        get().removeGroup(blockId, event.error);
        break;
      }
      case "artifact_started": {
        const { artifact_id, artifact_name, artifact_metadata, is_verified } =
          event;
        get().addGroup(artifact_id, {
          id: artifact_id,
          type: "artifact",
          artifact_id,
          artifact_name,
          artifact_metadata,
          is_verified,
        });
        break;
      }
      case "artifact_finished": {
        const { artifact_id } = event;
        get().removeGroup(artifact_id, event.error);
        break;
      }
      case "task_started": {
        const { task_id, task_name, task_metadata } = event;
        get().upsertBlockToStack(task_id, {
          type: "task",
          task_name,
          task_metadata,
        });
        break;
      }
      case "task_metadata": {
        const { task_id, metadata } = event;
        get().upsertBlockToStack(task_id, {
          type: "task",
          task_metadata: metadata,
        } as TaskContent);
        break;
      }
      case "task_finished": {
        const { task_id } = event;
        get().removeBlockStack(task_id, event.error);
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
  },
});
