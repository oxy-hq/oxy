import { css } from "styled-system/css";
import { TextFieldInput, TextFieldRoot } from "@/components/ui/Form/TextField";
import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";
import { TextFieldWithLabel } from "./TextFieldWithLabel";

export const ValuesField = (props) => {
  if (typeof props.value === "string") {
    return <ValuesInput {...props} />;
  }
  return <ValuesList {...props} />;
};

const ValuesInput = (props) => {
  return (
    <div>
      <TextFieldWithLabel label="Values" {...props} />
    </div>
  );
};

export const ListContainer: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  return <div>{children}</div>;
};

const ValuesList: React.FC = ({ value, onChange }) => {
  const onAdd = () => {
    onChange([...value, ""]);
  };

  const onRemove = (index) => () => {
    onChange(value.filter((_, i) => i !== index));
  };

  const onValueChange = (index) => (event) => {
    onChange(value.map((v, i) => (i === index ? event.target.value : v)));
  };

  return (
    <ListContainer>
      <ListHeader title="Values" onAdd={onAdd} />
      <ValuesListContent
        onRemove={onRemove}
        values={value}
        onValueChange={onValueChange}
      />
    </ListContainer>
  );
};

const ValuesListContent = ({ values, onRemove, onValueChange }) => {
  return (
    <div
      className={css({
        display: "flex",
        flexDirection: "column",
        gap: "gap.gapXS",
        bg: "#fff",
      })}
    >
      {values.map((value, i) => (
        <ValueItem
          value={value}
          onChange={onValueChange(i)}
          onRemove={onRemove(i)}
        />
      ))}
    </div>
  );
};

const ValueItem = ({
  value,
  onChange,
  onRemove,
}: {
  value: string;
  onChange: (value) => void;
  onRemove: () => void;
}) => {
  return (
    <div
      className={css({
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        gap: "sm",
      })}
    >
      <TextFieldRoot slotVariant="outline" className={css({ flex: 1 })}>
        <TextFieldInput value={value} onChange={onChange}></TextFieldInput>
      </TextFieldRoot>
      <Button variant="ghost" type="button" onClick={onRemove}>
        <Icon asset="remove_minus" />
      </Button>
    </div>
  );
};

const ListHeader = ({ title, onAdd }: { title: string; onAdd: () => void }) => {
  return (
    <div
      className={css({
        display: "flex",
        paddingBottom: "8px",
        justifyContent: "space-between",
        alignItems: "center",
      })}
    >
      <Text variant="body" weight="medium" size="small">
        {title}
      </Text>
      <Button
        content="icon"
        variant="ghost"
        type="button"
        data-functional
        onClick={onAdd}
      >
        <Icon asset="add" />
      </Button>
    </div>
  );
};
