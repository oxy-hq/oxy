import { css } from "styled-system/css";

import Text from "@/components/ui/Typography/Text";

const contentStyles = css({
  display: "flex",
  flex: "1",
  justifyContent: "center",
  alignItems: "center",
  h: "100%",
  w: "100%",
  borderColor: "border.secondary",
});

const EmptyPage = () => {
  return (
    <div className={contentStyles}>
      <Text variant="paragraph16Medium" color="primary">
        Select a file
      </Text>
    </div>
  );
};

export default EmptyPage;
