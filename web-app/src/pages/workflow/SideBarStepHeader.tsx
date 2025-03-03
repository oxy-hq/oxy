import { css } from "styled-system/css";
import useWorkflow from "@/stores/useWorkflow";
import Button from "@/components/ui/Button";
import Icon from "@/components/ui/Icon";
import Text from "@/components/ui/Typography/Text";

const SideBarStepHeader: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const node = useWorkflow((state) => state.getSelectedNode());
  const saveWorkflow = useWorkflow((state) => state.saveWorkflow);
  const moveStepUp = useWorkflow((state) => state.moveTaskUp);
  const moveStepDown = useWorkflow((state) => state.moveTaskDown);

  if (!node) {
    return null;
  }

  const handleMoveStepUp = () => {
    moveStepUp(node.data.id);
    saveWorkflow();
  };

  const handleMoveStepDown = () => {
    moveStepDown(node.data.id);
    saveWorkflow();
  };

  return (
    <div
      className={css({
        padding: "16px",
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
      })}
    >
      <Text variant="panelTitleRegular">{children}</Text>
      <div
        className={css({
          display: "flex",
          alignItems: "center",
        })}
      >
        <Button
          variant="ghost"
          type="button"
          disabled={!node.data.canMoveUp}
          className={css({ padding: 0 })}
          onClick={handleMoveStepUp}
        >
          <Icon asset="arrow_up_md" />
        </Button>
        <Button
          variant="ghost"
          type="button"
          disabled={!node.data.canMoveDown}
          className={css({ padding: 0 })}
          onClick={handleMoveStepDown}
        >
          <Icon asset="arrow_down_md" />
        </Button>
      </div>
    </div>
  );
};

export default SideBarStepHeader;
