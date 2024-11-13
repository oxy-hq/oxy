import AgentAvatar from "@/components/ui/AgentAvatar";
import Text from "@/components/ui/Typography/Text";
import { css } from "styled-system/css";
import { hstack, vstack } from "styled-system/patterns";

const avatarHeaderStyle = vstack({
  gap: "xl",
  pt: {
    base: "60px",
    smDown: "5xl"
  },
  maxW: "360px",
  alignSelf: "center",
  flex: 1
});

const linkStyles = css({
  _hover: {
    textDecorationLine: "underline"
  }
});

export default function AgentInfo({ agentName }: { agentName: string }) {
  return (
    <div className={avatarHeaderStyle}>
      <AgentAvatar
        name={agentName}
        className={css({
          textStyle: "label16Medium",
          color: "text.contrast",
          width: 70!,
          height: 70!,
          bg: "surface.contrast"
        })}
      />
      <div className={vstack({ gap: "md", textAlign: "center" })}>
        <div className={vstack({ gap: "xs" })}>
          <div
            className={hstack({
              gap: "sm"
            })}
          >
            <Text variant='headline20Medium' color='primary' className={linkStyles}>
              {agentName}
            </Text>
          </div>

          <Text variant='label14Regular' color='secondary'>
            Default data agent
          </Text>
        </div>
      </div>
    </div>
  );
}

