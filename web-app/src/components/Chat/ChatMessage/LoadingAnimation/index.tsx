import { css } from "styled-system/css";

const dotFlashingStyles = css({
  marginLeft: "6px",
  position: "relative",
  width: "4px",
  height: "4px",
  borderRadius: "2px",
  backgroundColor: "{colors.light-grey.3}",
  animation: "dotFlashing 1s infinite linear alternate",
  animationDelay: "0.5s",
  "&::before, &::after": {
    content: '""',
    display: "inline-block",
    position: "absolute",
    top: "0",
  },
  "&::before": {
    left: "-6px",
    width: "4px",
    height: "4px",
    borderRadius: "2px",
    backgroundColor: "{colors.light-grey.3}",
    animation: "dotFlashing 1s infinite alternate",
    animationDelay: "0s",
  },
  "&::after": {
    left: "6px",
    width: "4px",
    height: "4px",
    borderRadius: "2px",
    backgroundColor: "{colors.light-grey.3}",
    animation: "dotFlashing 1s infinite alternate",
    animationDelay: "1s",
  },
});

function LoadingAnimation() {
  return <div className={dotFlashingStyles}></div>;
}

export default LoadingAnimation;
