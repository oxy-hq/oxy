import { css } from "styled-system/css";

import Home from "@/components/Home";

const contentStyles = css({
  marginTop: {
    base: "52px",
    sm: "none",
  },
  display: "flex",
  flex: "1",
  minW: "0",
  overflow: "hidden",
  backgroundColor: "background.secondary",
  boxShadow: "secondary",
  borderLeft: "1px solid",
  borderColor: "border.primary",
});

const HomePage = () => {
  return (
    <div className={contentStyles}>
      <Home />
    </div>
  );
};

export default HomePage;
