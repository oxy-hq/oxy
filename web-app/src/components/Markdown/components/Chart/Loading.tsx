import { Player } from "@lottiefiles/react-lottie-player";
import animationData from "./lotties/chart.json";

const ChartLoading = () => {
  return (
    <div className='flex h-[400px] w-full flex-col items-center justify-center gap-4 p-4'>
      <Player loop autoplay src={animationData} className='max-h-[200px] max-w-[200px]' />
      <p className='text-muted-foreground text-sm'>Loading chart data...</p>
    </div>
  );
};

export default ChartLoading;
