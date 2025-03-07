import { css } from "styled-system/css";
import Text from "@/components/ui/Typography/Text";

const ExportFieldLabel: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  return (
    <Text
      variant="body"
      size="small"
      color="light"
      weight="regular"
      className={css({ flex: 1, boxSizing: "border-box", width: "100%" })}
    >
      {children}
    </Text>
  );
};

export default ExportFieldLabel;
