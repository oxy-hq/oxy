import { Home } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface ActionButtonProps {
  onClick: () => void;
}

const ActionButton = ({ onClick }: ActionButtonProps) => (
  <div className="text-center">
    <Button
      onClick={onClick}
      size="lg"
      className="px-8 py-3 text-lg font-semibold bg-blue-600 hover:bg-blue-700 text-white rounded-lg shadow-lg hover:shadow-xl transition-all duration-300"
    >
      <Home className="h-5 w-5 mr-2" />
      Start Using Oxy
    </Button>
  </div>
);

export default ActionButton;
