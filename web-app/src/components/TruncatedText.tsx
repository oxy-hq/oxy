import { cx } from "class-variance-authority";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./ui/shadcn/tooltip";

interface TruncatedTextProps {
  children: React.ReactNode;
  className?: string;
}

const TruncatedText = ({ children, className }: TruncatedTextProps) => {
  return (
    <TooltipProvider>
      <Tooltip delayDuration={500}>
        <TooltipTrigger className={cx("truncate min-w-0", className)}>
          {children}
        </TooltipTrigger>
        <TooltipContent>{children}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};

export default TruncatedText;
