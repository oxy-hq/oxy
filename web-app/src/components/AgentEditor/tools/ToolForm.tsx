import {
  Modal,
  ModalTrigger,
  ModalContent,
  ModalHeader,
} from "@/components/ui/Modal";
import {
  ExecuteSqlTool,
  RetrievalTool,
  ToolConfig,
  ValidateSqlTool,
} from "../type";
import ExecuteSqlToolForm from "./ExecuteSqlToolForm";
import ValidateSqlToolForm from "./ValidateSqlToolForm";
import RetrievalToolForm from "./RetrievalToolForm";
import { modalContentStyles, modalHeaderStyles } from "../styles";

const toolNameMap = {
  execute_sql: "execute sql",
  validate_sql: "validate sql",
  retrieval: "retrieval",
};

const ToolForm = ({
  trigger,
  value,
  type,
  open,
  onUpdate,
  onOpenChange,
}: {
  trigger?: React.ReactNode;
  value?: ToolConfig | null;
  type: "execute_sql" | "validate_sql" | "retrieval";
  open: boolean;
  onUpdate: (data: ToolConfig) => void;
  onOpenChange: (open: boolean) => void;
}) => {
  const handleUpdate = (data: ToolConfig) => {
    onUpdate(data);
    onOpenChange(false);
  };

  const onCancel = () => {
    onOpenChange(false);
  };

  return (
    <Modal open={open} onOpenChange={onOpenChange}>
      <ModalTrigger asChild>{trigger}</ModalTrigger>
      <ModalContent className={modalContentStyles}>
        <ModalHeader className={modalHeaderStyles}>
          {value ? "Update" : "Add"} {toolNameMap[type]} tool
        </ModalHeader>
        {type === "execute_sql" && (
          <ExecuteSqlToolForm
            value={value as ExecuteSqlTool}
            onUpdate={handleUpdate}
            onCancel={onCancel}
          />
        )}
        {type === "validate_sql" && (
          <ValidateSqlToolForm
            value={value as ValidateSqlTool}
            onUpdate={handleUpdate}
            onCancel={onCancel}
          />
        )}
        {type === "retrieval" && (
          <RetrievalToolForm
            value={value as RetrievalTool}
            onUpdate={handleUpdate}
            onCancel={onCancel}
          />
        )}
      </ModalContent>
    </Modal>
  );
};

export default ToolForm;
