import { useState } from "react";
import { AgentContext } from "@/components/AgentEditor/type";
import ContextForm from "./ContextForm";

const ContextModal = ({
  trigger,
  value,
  type,
  onUpdate,
}: {
  trigger: React.ReactNode;
  value?: AgentContext | null;
  type: "file" | "semantic_model";
  onUpdate: (data: AgentContext) => void;
  onTrigger?: () => void;
}) => {
  const [open, setOpen] = useState(false);

  const handleUpdate = (data: AgentContext) => {
    onUpdate(data);
    setOpen(false);
  };

  return (
    <ContextForm
      trigger={trigger}
      value={value}
      type={type}
      onUpdate={handleUpdate}
      open={open}
      onOpenChange={setOpen}
    />
  );
};

export default ContextModal;
