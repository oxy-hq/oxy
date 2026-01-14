import { Link } from "react-router-dom";
import { AlertCircle, ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

interface ErrorPageProps {
  message: string;
  description: string;
}

export function ErrorPage({ message, description }: ErrorPageProps) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-4">
      <AlertCircle className="h-12 w-12 text-destructive" />
      <div className="text-lg font-medium">{message}</div>
      <div className="text-muted-foreground">{description}</div>
      <Button variant="outline" asChild>
        <Link to="/traces">
          <ArrowLeft className="h-4 w-4 mr-2" />
          Back to Traces
        </Link>
      </Button>
    </div>
  );
}
