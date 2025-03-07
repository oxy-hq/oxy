import { css, cx } from "styled-system/css";
import {
  SelectContent,
  SelectItem,
  SelectRoot,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/Form/SelectField";
import { SelectPortal, SelectViewport } from "@radix-ui/react-select";
import Text from "@/components/ui/Typography/Text";

interface SelectOption {
  label: string;
  value: string;
}

interface SelectProps {
  options: SelectOption[];
  className?: string;
}

const Select: React.FC<SelectProps> = ({ options, className, ...props }) => {
  const containerStyles = css({
    boxSizing: "border-box",
    backgroundColor: "rgba(0, 0, 0, 0.02)",
    borderRadius: "rounded",
    flex: 1,
  });

  return (
    <div className={cx(containerStyles, className)}>
      <div className={css({ padding: "sm" })}>
        <SelectRoot {...props}>
          <SelectTrigger
            className={css({
              backgroundColor: "transparent",
              padding: "0",
              boxShadow: "none",
              "--default-border-shadow": "none",
              paddingLeft: "0",
              paddingRight: 0,
              "&[data-state=open]": {
                "--default-border-shadow": "none",
                boxShadow: "none",
              },
              "&:hover": {
                "--default-border-shadow": "none",
                boxShadow: "none",
              },
              "&:focus": {
                "--default-border-shadow": "none",
                boxShadow: "none",
              },
            })}
          >
            <Text variant="body" size="base" weight="regular">
              <SelectValue />
            </Text>
          </SelectTrigger>

          <SelectPortal>
            <SelectContent
              className={css({
                outline: "none",
              })}
            >
              <SelectViewport
                className={css({
                  backgroundColor: "#f3f3f3",
                  borderRadius: "8px",
                })}
              >
                {options.map((option) => (
                  <SelectItem className={css({})} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectViewport>
            </SelectContent>
          </SelectPortal>
        </SelectRoot>
      </div>
    </div>
  );
};

export default Select;
