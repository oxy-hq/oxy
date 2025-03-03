import { css } from "styled-system/css";

type Props = {
  children: React.ReactNode;
};
const ExportFieldContainer = ({ children }: Props) => {
  return (
    <div
      className={css({
        display: "flex",
        width: "100%",
        justifyContent: "space-between",
        alignItems: "center",
        boxSizing: "border-box",
      })}
    >
      {children}
    </div>
  );
};

export default ExportFieldContainer;
