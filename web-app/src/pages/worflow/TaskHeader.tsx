import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";

import { TaskData } from ".";
import { css } from "styled-system/css";
import { SvgAssets } from "@/components/ui/Icon/Dictionary";

const taskNameMap: Record<string, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
};

const taskIconMap: Record<string, SvgAssets> = {
  execute_sql: "code",
  loop_sequential: "arrow_reload",
  formatter: "placeholder",
  agent: "agent",
};

type Props = {
  task: TaskData;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const TaskHeader = ({
  task,
  expandable,
  expanded,
  onExpandClick,
}: Props) => {
  const taskName = taskNameMap[task.type as keyof typeof taskNameMap];
  const taskIcon = taskIconMap[task.type as keyof typeof taskIconMap];
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
              style={{ padding: 0 }}
              variant="ghost"
              size="small"
              onClick={onExpandClick}
            >
              <Icon asset={expanded ? "collapse" : "expand"} />
            </Button>
          )}
          <Icon asset="more_vertical" />
        </div>
      </div>
    </div>
  );
};
