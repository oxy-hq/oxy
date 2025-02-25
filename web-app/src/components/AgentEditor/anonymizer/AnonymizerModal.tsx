import { useState } from "react";
import { Modal, ModalHeader, ModalTrigger } from "@/components/ui/Modal";
import { ModalContent } from "@/components/ui/Modal";
import AnonymizerForm from "./AnonymizerForm";
import { AnonymizerConfig } from "@/components/AgentEditor/type";
import { modalContentStyles, modalHeaderStyles } from "../styles";

const AnonymizerModal = ({
  trigger,
  value,
  onUpdate,
}: {
  trigger: React.ReactNode;
  value?: AnonymizerConfig | null;
  onUpdate: (data: AnonymizerConfig) => void;
}) => {
  const [open, setOpen] = useState(false);

  const handleUpdate = (data: AnonymizerConfig) => {
    onUpdate(data);
    setOpen(false);
  };

  return (
    <Modal open={open} onOpenChange={setOpen}>
      <ModalTrigger asChild>{trigger}</ModalTrigger>
      <ModalContent className={modalContentStyles}>
        <ModalHeader className={modalHeaderStyles}>
          {value ? "Update" : "Add"} anonymizer
        </ModalHeader>
        <AnonymizerForm
          value={value}
          onUpdate={handleUpdate}
          onCancel={() => setOpen(false)}
        />
      </ModalContent>
    </Modal>
  );
};

export default AnonymizerModal;
