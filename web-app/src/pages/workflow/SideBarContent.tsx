import { css } from "styled-system/css";

type Props = {
  children: React.ReactNode;
};

const SideBarContent = ({ children }: Props) => {
  return (
    <div
      className={css({
        padding: "16px",
        display: "flex",
        flexDirection: "column",
        gap: "16px",
      })}
    >
      {children}
    </div>
  );
};

export default SideBarContent;
