import { css, cx } from "styled-system/css";

const spinnerStyles = css({
  width: "20px",
  aspectRatio: "1",
  borderRadius: "50%",
  border: "4px solid token(colors.surface.secondary)",
  borderRightColor: "border.secondary",
  animation: "rotate 1s infinite linear",
});

interface Props {
  className?: string;
}

export default function Spinner({ className }: Props) {
  return <div className={cx(spinnerStyles, className)} />;
}
