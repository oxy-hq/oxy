import { Loader2 } from "lucide-react";
import animationData from "./lotties/success.json";
import { Player } from "@lottiefiles/react-lottie-player";

type Props = {
  title: string;
  isCompleted: boolean;
};

export default function Step({ title, isCompleted }: Props) {
  return (
    <li className="flex gap-2 items-center">
      <div className="flex justify-center items-center">
        {isCompleted ? <SuccessIndicator /> : <Spinner />}
      </div>
      <span className="text-muted-foreground">{title}</span>
    </li>
  );
}

export const SuccessIndicator = () => {
  return (
    <div className="w-[20px] h-[20px] relative">
      <Player
        autoplay
        speed={3}
        keepLastFrame
        src={animationData}
        className="w-[60px] h-[60px] absolute top-[-20px] left-[-20px]"
      />
    </div>
  );
};

const Spinner = () => {
  return <Loader2 className="animate-spin h-[20px] w-[20px]" />;
};
