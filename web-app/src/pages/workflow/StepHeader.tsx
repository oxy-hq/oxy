import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";

import { css } from "styled-system/css";
import useWorkflow, { TaskConfigWithId } from "@/stores/useWorkflow";
import { taskIconMap, taskNameMap } from "./utils";

type Props = {
  task: TaskConfigWithId;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const StepHeader = ({
  task,
  expandable,
  expanded,
  onExpandClick,
}: Props) => {
  const taskName = taskNameMap[task.type];
  const taskIcon = taskIconMap[task.type];
  const setSelectedNodeId = useWorkflow((state) => state.setSelectedNodeId);
  const onMoreClick = () => {
    setSelectedNodeId(task.id);
  };
  return (
    <div
      className={css({
        gap: "sm",
        alignItems: "center",
        display: "flex",
      })}
    >
      <div
        className={css({
          display: "flex",
          alignItems: "center",
        })}
      >
        <div
          className={css({
            display: "flex",
            alignContent: "center",
            justifyContent: "center",
            padding: "10px",
            background: "#F5F5F5",
            borderRadius: "8px",
          })}
        >
          <Icon asset={taskIcon} />
        </div>
      </div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          flex: 1,
        }}
      >
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            gap: "4px",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
            }}
          >
            <Text variant="label12Medium" color="lessContrast">
              {taskName}
            </Text>
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
            }}
          >
            <Text variant="label14Medium">{task.name}</Text>
          </div>
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            height: "100%",
          }}
        >
          {expandable && (
            <Button
              className={css({
                padding: "padding.paddingXXS",
              })}
              variant="ghost"
              size="small"
              onClick={onExpandClick}
            >
              <Icon asset={expanded ? "collapse" : "expand"} />
            </Button>
          )}
          <Button
            className={css({
              padding: "padding.paddingXXS",
            })}
            variant="ghost"
            size="small"
            onClick={onMoreClick}
          >
            <Icon asset="more_vertical" />
          </Button>
        </div>
      </div>
    </div>
  );
};
