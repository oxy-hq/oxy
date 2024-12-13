import dayjs from "dayjs";
import { NavLink } from "react-router-dom";
import { css, cx } from "styled-system/css";
import { hstack, vstack } from "styled-system/patterns";

import Text from "@/components/ui/Typography/Text";
import { getAgentNameFromPath } from "@/libs/utils/agent";
import { formatDateToHumanReadable } from "@/libs/utils/date";
import { Agent } from "@/types/chat";

import AgentAvatar from "../ui/AgentAvatar";

const cardStyles = css({
  display: "flex",
  gap: "md",
  width: "100%",
  cursor: "pointer",
  borderRadius: "rounded",
  pr: "md",
  _hover: {
    bg: "background.opacity"
  },
  flexDirection: "column",
  height: "auto",
  padding: "xs",
  sm: {
    flexDirection: "row",
    height: "59px",
    pr: "sm",
    py: "xxs",
    pl: "xxs"
  }
});

const skeletonStyles = css({
  rightSlideAnimation: true,
  borderRadius: "full!",
  overflow: "hidden",
  "@media (max-width: 768px)": {
    width: "45px!",
    height: "45px!"
  }
});

export function AgentCardSkeleton({ className }: { className?: string }) {
  return (
    <div className={cx(cardStyles, css({ boxShadow: "none!" }), className)}>
      <div
        className={cx(
          skeletonStyles,
          css({
            width: 55,
            height: 55
          })
        )}
      />
      <div
        className={vstack({
          justifyContent: "space-between",
          alignItems: "start",
          flex: 1
        })}
      >
        <div className={cx(skeletonStyles, css({ width: "50%", height: 17 }))} />
        <div className={cx(skeletonStyles, css({ width: "100%", height: 17 }))} />
      </div>
    </div>
  );
}

export function AgentCard({ agent }: { agent: Agent }) {
  const agentName = getAgentNameFromPath(agent.path);
  return (
    <NavLink to={`/chat/${btoa(agent.path)}`} className={cardStyles} key={agent.path}>
      <AgentAvatar
        name={agentName}
        className={css({
          textStyle: "label14Medium",
          color: "text.contrast",
          width: 55!,
          height: 55!,
          bg: "surface.contrast"
        })}
      />
      <div
        className={hstack({
          flex: 1,
          flexDirection: "column",
          alignItems: "start",
          minWidth: 0,
          sm: {
            justifyContent: "space-between",
            alignItems: "center",
            flexDirection: "row"
          }
        })}
      >
        <div
          className={vstack({
            gap: "xs",
            alignItems: "start",
            flex: 3,
            minW: 0,
            w: "100%",
            sm: {
              w: "unset"
            }
          })}
        >
          <Text
            variant='label14Medium'
            color='primary'
            className={css({
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              display: "block",
              _hover: {
                textDecoration: "underline"
              }
            })}
          >
            {agentName}
          </Text>
          <Text
            variant='paragraph14Regular'
            className={css({
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              display: "block",
              w: "100%"
            })}
            color='secondary'
          >
            {agent.description}
          </Text>
        </div>

        <Text variant='paragraph12Regular' color='secondary'>
          Updated {formatDateToHumanReadable(dayjs(agent.updated_at).format())} ago
        </Text>
      </div>
    </NavLink>
  );
}
