import type { ReactNode } from "react";

export function CenteredLayout({ children }: { children: ReactNode }) {
  return (
    <div className='flex min-h-screen w-full items-center justify-center bg-background p-4'>
      {children}
    </div>
  );
}
