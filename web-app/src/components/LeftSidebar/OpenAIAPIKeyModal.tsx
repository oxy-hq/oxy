import { css } from "styled-system/css";
import { toast } from "@/components/ui/Toast";
import { service } from "@/services/service";
import { useEffect, useState } from "react";
import { ModalContent, ModalFooter, ModalHeader } from "@/components/ui/Modal";
import { TextFieldInput } from "@/components/ui/Form/TextField";
import { Modal } from "@/components/ui/Modal";
import Button from "@/components/ui/Button";

const modalHeaderStyles = css({
  w: "100%",
  p: "xl",
  position: "relative",
  boxShadow: "inset 0 -1px 0 0 token(colors.border.primary)",
});

export default function OpenAIAPIKeyModal({
  open,
  setOpen,
}: {
  open: boolean;
  setOpen: (open: boolean) => void;
}) {
  const [openaiApiKey, setOpenaiApiKey] = useState("");

  useEffect(() => {
    service
      .getOpenaiApiKey()
      .then((key) => {
        if (!key) {
          setOpen(true);
        }
        setOpenaiApiKey(key);
        return key;
      })
      .catch((error) => {
        toast({
          title: "Error",
          description: error.message,
        });
      });
  }, [open, setOpen]);

  return (
    <Modal open={open} onOpenChange={setOpen}>
      <ModalContent className={css({ w: "400px" })}>
        <ModalHeader className={modalHeaderStyles}>Open AI API key</ModalHeader>
        <p className={css({ color: "text.light", p: "xl" })}>
          <TextFieldInput
            placeholder="Enter your Open AI API key here..."
            value={openaiApiKey}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
              setOpenaiApiKey(e.target.value)
            }
          />
        </p>
        <ModalFooter className={css({ p: "xl" })}>
          <Button
            variant="primary"
            content="text"
            onClick={async () => {
              await service.setOpenaiApiKey(openaiApiKey);
              setOpen(false);
            }}
          >
            Connect
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
