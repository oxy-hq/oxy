import { Loader2, Sparkles } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";

interface InsightsCardProps {
  mostPopular: string | null;
  mostPopularCount: number;
  totalUsage: number;
  agentPercentage: number;
  isLoading: boolean;
}

export default function InsightsCard({
  mostPopular,
  mostPopularCount,
  totalUsage,
  agentPercentage,
  isLoading
}: InsightsCardProps) {
  const popularPercentage =
    mostPopularCount && totalUsage > 0 ? Math.round((mostPopularCount / totalUsage) * 100) : 0;

  return (
    <Card className='border-primary/20 bg-gradient-to-br from-primary/5 to-primary/10'>
      <CardHeader className='pb-2'>
        <CardTitle className='flex items-center gap-2 text-base'>
          <Sparkles className='h-4 w-4 text-primary' />
          Insights
        </CardTitle>
      </CardHeader>
      <CardContent className='space-y-2 text-sm'>
        {isLoading ? (
          <div className='flex items-center justify-center py-4'>
            <Loader2 className='h-5 w-5 animate-spin text-muted-foreground' />
          </div>
        ) : (
          <>
            {mostPopular && (
              <p className='text-muted-foreground'>
                <span className='font-medium text-foreground'>{mostPopular}</span> is your most
                queried metric, appearing in {popularPercentage}% of all analytics requests.
              </p>
            )}
            <p className='text-muted-foreground'>
              {agentPercentage > 50
                ? "Agent is the primary access method, suggesting strong adoption of conversational analytics."
                : "Workflows and tasks are frequently used to access metrics programmatically."}
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}
