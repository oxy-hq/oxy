import type { ReactNode } from "react";

/**
 * The page frame every partner page uses — identical to the admin page frame
 * (`mx-auto max-w-7xl` + the eyebrow/title header), so the two surfaces read as
 * one product.
 */
export default function PageShell({
  eyebrow,
  title,
  description,
  actions,
  children,
  testId
}: {
  eyebrow: string;
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
  testId?: string;
}) {
  return (
    <div className='mx-auto max-w-7xl space-y-5 p-6 lg:px-10 lg:py-8' data-testid={testId}>
      <header className='flex items-start justify-between gap-4'>
        <div>
          <div className='flex items-baseline gap-3'>
            <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.18em]'>
              {eyebrow}
            </p>
            <span className='text-muted-foreground/40'>/</span>
            <h1 className='font-semibold text-xl tracking-tight'>{title}</h1>
          </div>
          {description && <p className='mt-1 text-muted-foreground text-sm'>{description}</p>}
        </div>
        {actions && <div className='flex shrink-0 items-center gap-2'>{actions}</div>}
      </header>
      {children}
    </div>
  );
}
