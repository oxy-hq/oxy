import { Loader2, ChevronLeft } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface Props {
  children: React.ReactNode;
  actions?: React.ReactNode;
  title: string;
  loading?: boolean;
  onBack?: () => void;
}

const PageWrapper = ({ children, title, actions, loading, onBack }: Props) => {
  return (
    <div className="p-4 space-y-6">
      <div className="pb-2 flex items-center border-b border-border justify-between">
        <div className="flex items-center gap-2">
          {onBack && (
            <Button
              variant="ghost"
              size="icon"
              onClick={onBack}
              className="h-9 w-9"
            >
              <ChevronLeft className="h-5 w-5" />
            </Button>
          )}
          <h3 className="text-xl h-9 content-center">{title}</h3>
        </div>

        {!loading && actions}
      </div>
      <div>
        {!loading ? (
          children
        ) : (
          <div className="flex items-center justify-center h-30">
            <Loader2 className="animate-spin h-6 w-6" />
          </div>
        )}
      </div>
    </div>
  );
};

export default PageWrapper;
