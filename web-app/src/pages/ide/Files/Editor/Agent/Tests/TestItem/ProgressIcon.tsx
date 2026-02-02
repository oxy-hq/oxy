const ProgressIcon = ({
  progress,
  total,
}: {
  progress: number;
  total: number;
}) => {
  const percentage = (progress / total) * 100;

  return (
    <div className="relative w-4 h-4 p-[1px] border border-border rounded-full">
      <div className="w-full h-full rounded-full bg-muted-foreground/20">
        <div
          className="w-full h-full rounded-full transition-all duration-300 ease-in-out"
          style={{
            background: `conic-gradient(currentColor ${percentage}%, transparent ${percentage}%)`,
          }}
        />
      </div>
    </div>
  );
};

export default ProgressIcon;
