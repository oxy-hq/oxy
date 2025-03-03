import { css, cx } from "styled-system/css";
import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import ExportPathInput from "./ExportPathInput";
import ExportFormatSelect from "./ExportFormatSelect";
import { useFormContext } from "react-hook-form";
import { ExportConfig } from "@/stores/useWorkflow";

const ExportSection: React.FC = () => {
  const { watch, setValue } = useFormContext<{ export?: ExportConfig }>();
  const exp = watch("export");

  return (
    <div className={css({
      "_hover": {
        "& .removeButton": {
          visibility: "initial",
        }
      }
    })}>
      <div className={css({
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "16px"
      })}>
        <Text variant="paragraph14Regular">Export</Text>
        <Button
          variant="ghost"
          onClick={() => setValue("export", { path: "", format: "json" })}
          type="button"
          className={css({
            display: "flex",
            color: "black",
            padding: 0,
            visibility: exp ? "hidden" : "visible",
          })}
        >
          <Icon asset="add" />
        </Button>
        {exp && (
          <Button variant="ghost" type="button" className={cx(css({
            visibility: "hidden",
            padding: "0 8px",
            borderRadius: "4px",
            border: "1px solid",
          }), "removeButton")}
            onClick={() => setValue("export", undefined)}
          >
            <Text variant="button" weight="regular" >Remove</Text>
          </Button>
        )}
      </div>
      {exp && (
        <div
          className={css({
            gap: "gap.gapXS",
            display: "flex",
            flexDirection: "column",
            padding: "0 16px",
          })}
        >
          <ExportFormatSelect />
          <ExportPathInput />
        </div>

      )}
    </div>
  );
};

export default ExportSection;
