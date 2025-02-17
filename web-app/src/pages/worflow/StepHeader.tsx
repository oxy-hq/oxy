import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";

import { StepData } from ".";
import { css } from "styled-system/css";
import { SvgAssets } from "@/components/ui/Icon/Dictionary";

const stepNameMap: Record<string, string> = {
  execute_sql: "SQL",
  loop_sequential: "Loop sequential",
  formatter: "Formatter",
  agent: "Agent",
};

const stepIconMap: Record<string, SvgAssets> = {
  execute_sql: "code",
  loop_sequential: "arrow_reload",
  formatter: "placeholder",
  agent: "agent",
};

type Props = {
  step: StepData;
  expandable?: boolean;
  expanded?: boolean;
  onExpandClick?: () => void;
};

export const StepHeader = ({
  step,
  expandable,
  expanded,
  onExpandClick,
}: Props) => {
  const stepName = stepNameMap[step.type as keyof typeof stepNameMap];
  const stepIcon = stepIconMap[step.type as keyof typeof stepIconMap];
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
          <Icon asset={stepIcon} />
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
              {stepName}
            </Text>
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
            }}
          >
            <Text variant="label14Medium">{step.name}</Text>
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
