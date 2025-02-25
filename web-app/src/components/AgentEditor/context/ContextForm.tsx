import { Modal, ModalHeader, ModalTrigger } from "@/components/ui/Modal";
import { ModalContent } from "@/components/ui/Modal";
import {
  AgentContext,
  AgentContextFile,
  AgentContextSemanticModel,
} from "@/components/AgentEditor/type";
import AgentContextFileForm from "./AgentContextFileForm";
import AgentContextSemanticModelForm from "./AgentContextSemanticModelForm";
import { modalContentStyles, modalHeaderStyles } from "../styles";

const ContextForm = ({
  trigger,
  value,
  type,
  onUpdate,
  open,
  onOpenChange,
}: {
  trigger?: React.ReactNode;
  value?: AgentContext | null;
  type: "file" | "semantic_model";
  onUpdate: (data: AgentContext) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) => {
  const handleUpdate = (data: AgentContext) => {
    onUpdate(data);
    onOpenChange(false);
  };

  return (
    <Modal open={open} onOpenChange={onOpenChange}>
      <ModalTrigger asChild>{trigger}</ModalTrigger>
      <ModalContent className={modalContentStyles}>
        <ModalHeader className={modalHeaderStyles}>
          {value ? "Update" : "Add"}{" "}
          {type === "file" ? "file" : "semantic model"} context
        </ModalHeader>
        {type === "file" && (
          <AgentContextFileForm
            value={value as AgentContextFile}
            onUpdate={handleUpdate}
            onCancel={() => onOpenChange(false)}
          />
        )}
        {type === "semantic_model" && (
          <AgentContextSemanticModelForm
            value={value as AgentContextSemanticModel}
            onUpdate={handleUpdate}
            onCancel={() => onOpenChange(false)}
          />
        )}
      </ModalContent>
    </Modal>
  );
};

export default ContextForm;
