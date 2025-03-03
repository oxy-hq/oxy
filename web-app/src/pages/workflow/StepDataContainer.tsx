import React, { ReactNode } from "react";
import { css } from "styled-system/css";

type StepDataContainerProps = {
  children: ReactNode;
};

const StepDataContainer: React.FC<StepDataContainerProps> = ({ children }) => {
  return (
    <div
      className={css({
        padding: "16px",
        display: "flex",
        flexDirection: "column",
        gap: "16px",
        borderBottom: "1px solid",
        borderColor: "neutral.border.colorBorderSecondary",
      })}
    >
      {children}
    </div>
  );
};

export default StepDataContainer;
