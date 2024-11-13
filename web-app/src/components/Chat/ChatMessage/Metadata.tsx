import { css } from "styled-system/css";
import { hstack } from "styled-system/patterns";

import Text from "@/components/ui/Typography/Text";
import { formatDate } from "@/libs/utils/date";
import AgentAvatar from "@/components/ui/AgentAvatar";

const timeStyles = css({
  color: "text.secondary"
});

const containerStyles = hstack({
  gap: "sm",
  color: "text.light"
});

const linkStyles = css({
  _hover: {
    textDecorationLine: "underline"
  }
});

type Props = {
  time?: Date;
  agentName?: string;
};

const HOUR_FORMAT = "h:mma";

function Metadata({ time, agentName }: Props) {
  return (
    <div className={containerStyles}>
      <AgentAvatar
        name={agentName ?? ""}
        className={css({
          textStyle: "paragraph10Regular",
          color: "text.contrast",
          width: 20!,
          height: 20!,
          bg: "surface.contrast"
        })}
      />
      <div className={hstack({ gap: "sm" })}>
        <Text className={linkStyles} variant='label14Medium'>
          {agentName || "Agent"}
        </Text>
      </div>

      {time && (
        <Text className={timeStyles} variant='label14Regular'>
          {formatDate(time, HOUR_FORMAT)}
        </Text>
      )}
    </div>
  );
}

export default Metadata;

