import { GitBranch } from "lucide-react";

const RoutePanel = ({ routeName }: { routeName: string }) => (
  <div className='flex h-full flex-col items-center justify-center gap-3 p-6 text-center'>
    <div className='flex h-10 w-10 items-center justify-center rounded-full bg-node-plan/10'>
      <GitBranch className='h-5 w-5 text-node-plan' />
    </div>
    <div>
      <p className='font-medium text-foreground text-sm'>Routed to automation</p>
      <p className='mt-1 font-mono text-muted-foreground text-xs'>{routeName}</p>
    </div>
  </div>
);

export default RoutePanel;
