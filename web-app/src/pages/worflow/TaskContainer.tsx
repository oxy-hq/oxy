type Props = {
  children: React.ReactNode;
};

export const TaskContainer = ({ children }: Props) => {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "8px",
        borderRadius: "10px",
        border: "1px solid #D4D4D4",
        background: "#FBFBFB",
        padding: "4px",
      }}
    >
      {children}
    </div>
  );
};
