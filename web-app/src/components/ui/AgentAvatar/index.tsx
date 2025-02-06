import * as RadixAvatar from "@radix-ui/react-avatar";
import { css, cx } from "styled-system/css";

import { upperFirst } from "@/libs/utils/string";

const containerStyles = css({
  display: "flex",
  justifyContent: "center",
  alignItems: "center",
  overflow: "hidden",
  flexShrink: 0,
  borderRadius: "full",
});

type Props = {
  name: string;
  className?: string;
};

export default function AgentAvatar({ name = "", className }: Props) {
  return (
    <RadixAvatar.Root className={cx(containerStyles, className)}>
      <RadixAvatar.Fallback>{upperFirst(name)}</RadixAvatar.Fallback>
    </RadixAvatar.Root>
  );
}
