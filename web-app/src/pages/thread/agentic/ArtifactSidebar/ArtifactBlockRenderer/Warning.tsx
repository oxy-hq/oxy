import { TriangleAlert } from "lucide-react";

function Warning({ message }: { message: string }) {
  return (
    <div className='m-4 flex items-center gap-2 rounded-md border border-yellow-300 bg-yellow-50 px-4 py-3 text-sm text-yellow-800'>
      <TriangleAlert className='h-5 w-5 shrink-0 text-yellow-500' />
      <span>{message}</span>
    </div>
  );
}

export default Warning;
