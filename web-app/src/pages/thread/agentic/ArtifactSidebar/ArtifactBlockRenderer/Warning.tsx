import { TriangleAlert } from "lucide-react";

function Warning({ message }: { message: string }) {
  return (
    <div className='m-4 flex items-center gap-2 rounded-md border border-warning bg-warning/10 px-4 py-3 text-sm text-warning'>
      <TriangleAlert className='h-5 w-5 shrink-0 text-warning' />
      <span>{message}</span>
    </div>
  );
}

export default Warning;
