import { hstack } from "styled-system/patterns";
import Button from "../ui/Button";

const ModalFormFooter = ({
  onCancel,
  isUpdate,
}: {
  onCancel: () => void;
  isUpdate: boolean;
}) => {
  return (
    <div className={hstack({ justifyContent: "flex-end", px: "xl", pb: "xl" })}>
      <Button type="button" variant="ghost" content="text" onClick={onCancel}>
        Cancel
      </Button>
      <Button type="submit" variant="primary" content="text">
        {isUpdate ? "Update" : "Add"}
      </Button>
    </div>
  );
};

export default ModalFormFooter;
