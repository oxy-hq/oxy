import animationData from "./lotties/chart.json";
import { Player } from "@lottiefiles/react-lottie-player";

const ChartLoading = () => {
  return (
    <div className="w-full h-[400px] flex flex-col gap-4 p-4 justify-center items-center">
      <Player
        loop
        autoplay
        src={animationData}
        className="max-w-[200px] max-h-[200px]"
      />
      <p className="text-sm text-muted-foreground">Loading chart data...</p>
    </div>
  );
};

export default ChartLoading;
