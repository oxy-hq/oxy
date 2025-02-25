import { useState } from "react";
import { ToolConfig } from "../type";
import ToolForm from "./ToolForm";

const ToolModal = ({
  trigger,
  value,
  type,
  onUpdate,
}: {
  trigger: React.ReactNode;
  value?: ToolConfig | null;
  type: "execute_sql" | "validate_sql" | "retrieval";
  onUpdate: (data: ToolConfig) => void;
}) => {
  const [open, setOpen] = useState(false);

  return (
    <ToolForm
      trigger={trigger}
      value={value}
      type={type}
      open={open}
      onUpdate={onUpdate}
      onOpenChange={setOpen}
    />
  );
};

export default ToolModal;
