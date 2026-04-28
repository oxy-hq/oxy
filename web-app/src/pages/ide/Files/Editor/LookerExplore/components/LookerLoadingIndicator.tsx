import useTheme from "@/stores/useTheme";

const OxyIcon = () => {
  const theme = useTheme((state) => state.theme);
  return (
    <div className='flex h-9 w-9 items-center justify-center rounded-lg border bg-background p-1.5 shadow-sm'>
      <img
        src={theme === "dark" ? "/oxygen-dark.svg" : "/oxygen-light.svg"}
        alt='Oxygen'
        className='h-full w-full'
      />
    </div>
  );
};

const LookerIcon = () => (
  <div className='flex h-9 w-9 items-center justify-center rounded-lg border bg-background p-1 shadow-sm'>
    <img src='/looker.svg' alt='Looker' className='h-full w-full' />
  </div>
);

const FlowDots = () => (
  <div className='flex items-center gap-1.5'>
    {[0, 1, 2].map((i) => (
      <span
        key={i}
        className='h-1.5 w-1.5 rounded-full bg-primary/60'
        style={{
          animation: `looker-flow-dot 1.2s ease-in-out infinite`,
          animationDelay: `${i * 0.2}s`
        }}
      />
    ))}
    <style>{`
      @keyframes looker-flow-dot {
        0%, 100% { opacity: 0.2; transform: translateX(0); }
        50%       { opacity: 1;   transform: translateX(4px); }
      }
    `}</style>
  </div>
);

interface LookerLoadingIndicatorProps {
  message?: string;
}

const LookerLoadingIndicator = ({ message }: LookerLoadingIndicatorProps) => (
  <div className='flex flex-col items-center gap-4'>
    <div className='flex items-center gap-3'>
      <OxyIcon />
      <FlowDots />
      <LookerIcon />
    </div>
    {message && <p className='text-muted-foreground text-sm'>{message}</p>}
  </div>
);

export default LookerLoadingIndicator;
