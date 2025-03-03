import { css } from "styled-system/css";
import Text from "@/components/ui/Typography/Text";

type FieldLabelProps = {
  children: React.ReactNode;
};

const FieldLabel: React.FC<FieldLabelProps> = ({ children }) => {
  return (
    <div
      className={css({
        height: "24px",
        flex: 1,
        display: "flex",
        alignItems: "center",
      })}
    >
      <Text variant="body" size="small" weight="regular">
        {children}
      </Text>
    </div>
  );
};

export default FieldLabel;
