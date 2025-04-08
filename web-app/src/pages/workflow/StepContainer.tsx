import { nodePadding } from "./constants";

type Props = {
  children: React.ReactNode;
  selected?: boolean;
  width?: number;
  height?: number;
};

export const StepContainer = ({ children, selected, width, height }: Props) => {
  return (
    <div
      className={`flex flex-col gap-2 rounded-[10px] border border-neutral-300 ${
        selected ? "bg-gray-200" : "bg-white"
      }`}
      style={{ padding: `${nodePadding}px`, width: width, height: height }}
    >
      {children}
    </div>
  );
};
