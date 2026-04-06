import { Player } from "@lottiefiles/react-lottie-player";
import { Spinner as SpinnerIcon } from "@/components/ui/shadcn/spinner";
import animationData from "./lotties/success.json";

type Props = {
  title: string;
  isCompleted: boolean;
};

export default function Step({ title, isCompleted }: Props) {
  return (
    <li className='flex items-center gap-2'>
      <div className='flex items-center justify-center'>
        {isCompleted ? <SuccessIndicator /> : <Spinner />}
      </div>
      <span className='text-muted-foreground'>{title}</span>
    </li>
  );
}

export const SuccessIndicator = () => {
  return (
    <div className='relative h-[20px] w-[20px]'>
      <Player
        autoplay
        speed={3}
        keepLastFrame
        src={animationData}
        className='absolute top-[-20px] left-[-20px] h-[60px] w-[60px]'
      />
    </div>
  );
};

const Spinner = () => {
  return <SpinnerIcon className='size-6' />;
};
