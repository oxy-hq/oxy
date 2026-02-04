const ProgressIcon = ({ progress, total }: { progress: number; total: number }) => {
  const percentage = (progress / total) * 100;

  return (
    <div className='relative h-4 w-4 rounded-full border border-border p-[1px]'>
      <div className='h-full w-full rounded-full bg-muted-foreground/20'>
        <div
          className='h-full w-full rounded-full transition-all duration-300 ease-in-out'
          style={{
            background: `conic-gradient(currentColor ${percentage}%, transparent ${percentage}%)`
          }}
        />
      </div>
    </div>
  );
};

export default ProgressIcon;
