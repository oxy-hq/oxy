import { ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface ScrollToBottomButtonProps {
  onClick: () => void;
  visible: boolean;
  className?: string;
}

/**
 * Reusable scroll-to-bottom button component
 * Displays a floating button that scrolls to the bottom of a container when clicked
 */
export function ScrollToBottomButton({
  onClick,
  visible,
  className = "",
}: ScrollToBottomButtonProps) {
  if (!visible) return null;

  return (
    <Button
      variant="outline"
      size="icon"
      onClick={onClick}
      className={`fixed bottom-24 right-8 rounded-full shadow-lg hover:shadow-xl transition-all duration-200 z-10 ${className}`}
      aria-label="Scroll to bottom"
    >
      <ArrowDown className="h-4 w-4" />
    </Button>
  );
}
