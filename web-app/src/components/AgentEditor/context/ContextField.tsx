import Text from "@/components/ui/Typography/Text";
import { AgentConfig } from "@/components/AgentEditor/type";
import { useFieldArray, useFormContext } from "react-hook-form";
import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import ContextModal from "./ContextModal";
import { hstack, vstack } from "styled-system/patterns";
import { listItemStyle } from "../styles";
import AddContextMenu from "./AddContextMenu";

const ContextField = () => {
  const { control } = useFormContext<AgentConfig>();

  const { fields, append, remove, update } = useFieldArray({
    control,
    name: "context",
  });

  return (
    <div className={vstack({ gap: "sm", alignItems: "stretch" })}>
      <div
        className={hstack({
          justifyContent: "space-between",
        })}
      >
        <Text variant="label14Medium" color="primary">
          Context
        </Text>
        <AddContextMenu onAddContext={append} />
      </div>

      {fields.map((field, index) => (
        <div key={field.id} className={hstack({ gap: "sm" })}>
          <ContextModal
            value={field}
            type={field.type}
            onUpdate={(data) => {
              update(index, data);
            }}
            trigger={
              <button className={listItemStyle}>
                <Text variant="label14Medium" color="primary">
                  {field.name}
                </Text>
                <Text variant="label14Medium" color="secondary">
                  {field.type}
                </Text>
              </button>
            }
          />
          <Button
            variant="ghost"
            content="icon"
            onClick={() => {
              remove(index);
            }}
          >
            <Icon asset="remove_minus" />
          </Button>
        </div>
      ))}
    </div>
  );
};

export default ContextField;
