import { AgentArtifact, Artifact, WorkflowArtifact } from "@/services/mock";
import { ArtifactValueContent } from "@/types/chat";

export const extractUpdatedValue = (
  updatedArtifact: Artifact,
  artifact_value: ArtifactValueContent,
) => {
  let updatedValue = {};

  switch (artifact_value.value.type) {
    case "log_item": {
      const output =
        (updatedArtifact as WorkflowArtifact).content.value.output ?? [];
      const lastItem = output[output.length - 1];

      if (artifact_value.value.value.append && lastItem.append) {
        output[output.length - 1] = {
          ...lastItem,
          content: `${lastItem.content}${artifact_value.value.value.content}`,
        };
      } else {
        output.push(artifact_value.value.value);
      }
      updatedValue = {
        output: [...output],
      };
      break;
    }
    case "content": {
      updatedValue = {
        output: `${(updatedArtifact as AgentArtifact).content.value.output ?? ""}${artifact_value.value.value}`,
      };
      break;
    }
    case "execute_sql": {
      updatedValue = artifact_value.value.value;
      break;
    }
    default: {
      break;
    }
  }

  return updatedValue;
};
