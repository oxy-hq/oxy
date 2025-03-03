import { nodePadding } from "./constants";

type Props = {
  children: React.ReactNode;
  selected?: boolean;
};

export const StepContainer = ({ children, selected }: Props) => {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        borderRadius: "10px",
        border: "1px solid #D4D4D4",
        backgroundColor: selected ? "#E5E7EB" : "#FFF",
        padding: `${nodePadding}px`,
      }}
    >
      {children}
    </div>
  );
};
