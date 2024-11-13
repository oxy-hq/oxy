import Icon from ".";
import { SvgAssets } from "./Dictionary";
import { css, cx } from "styled-system/css";

const frameStyles = css({
  w: "lg",
  h: "lg",
  p: "xxs"
});

const iconWrapperStyles = css({
  display: "flex",
  justifyContent: "center",
  alignItems: "center",
  w: "md",
  h: "md"
});

export default function LogoFrame({
  logo,
  className
}: {
  logo: SvgAssets;
  className?: string;
}) {
  return (
    <div className={cx(frameStyles, className)}>
      <div className={iconWrapperStyles}>
        <Icon asset={logo} size="fromIcon" />
      </div>
    </div>
  );
}
