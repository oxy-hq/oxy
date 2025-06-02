import { Button } from "@/components/ui/shadcn/button";

const ErrorState = ({ error }: { error: Error }) => {
  return (
    <div className="flex flex-col gap-4 p-6 items-center justify-center">
      <div className="text-red-500 text-center">
        <p className="text-lg font-semibold">Error loading threads</p>
        <p className="text-sm text-muted-foreground">
          {error?.message || "Something went wrong"}
        </p>
      </div>
      <Button variant="outline" onClick={() => window.location.reload()}>
        Try again
      </Button>
    </div>
  );
};

export default ErrorState;
