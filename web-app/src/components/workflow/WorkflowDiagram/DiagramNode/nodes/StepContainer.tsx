import { nodePadding } from "../../layout/constants";

type Props = {
  children: React.ReactNode;
  selected?: boolean;
  width?: number;
  height?: number;
};

export const StepContainer = ({ children, selected, width, height }: Props) => {
  return (
    <div
      className={`flex flex-col gap-2 rounded-[10px] border border-border ${
        selected ? "bg-accent" : "bg-card"
      }`}
      style={{ padding: `${nodePadding}px`, width: width, height: height }}
    >
      {children}
    </div>
  );
};
