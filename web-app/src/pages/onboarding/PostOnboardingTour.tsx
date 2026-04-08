import { AlertTriangle, CheckCircle2, Database, Key, Loader2, XCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import type { ReadinessResponse } from "@/services/api/onboarding";
import { OnboardingService } from "@/services/api/onboarding";

interface Props {
  projectType: "demo" | "new" | "github";
  onEnterApp: () => void;
}

interface TourCheckItemProps {
  icon: React.ReactNode;
  title: string;
  status: "ok" | "warning" | "loading";
  children: React.ReactNode;
}

function TourCheckItem({ icon, title, status, children }: TourCheckItemProps) {
  return (
    <div className='flex items-start gap-3 rounded-lg border p-4'>
      <div className='mt-0.5 text-muted-foreground'>{icon}</div>
      <div className='flex-1 space-y-2'>
        <div className='flex items-center gap-2'>
          <span className='font-medium text-sm'>{title}</span>
          {status === "loading" && (
            <Loader2 className='h-3.5 w-3.5 animate-spin text-muted-foreground' />
          )}
          {status === "ok" && <CheckCircle2 className='h-3.5 w-3.5 text-emerald-500' />}
          {status === "warning" && <AlertTriangle className='h-3.5 w-3.5 text-amber-500' />}
        </div>
        {children}
      </div>
    </div>
  );
}

function LlmKeyRow({ name, present }: { name: string; present: boolean }) {
  return (
    <div className='flex items-center gap-2'>
      {present ? (
        <CheckCircle2 className='h-3 w-3 shrink-0 text-emerald-500' />
      ) : (
        <XCircle className='h-3 w-3 shrink-0 text-muted-foreground/40' />
      )}
      <code
        className={`font-mono text-[11px] ${present ? "text-foreground" : "text-muted-foreground/50"}`}
      >
        {name}
      </code>
    </div>
  );
}

export function PostOnboardingTour({ projectType, onEnterApp }: Props) {
  const [readiness, setReadiness] = useState<ReadinessResponse | null>(null);

  useEffect(() => {
    OnboardingService.getReadiness()
      .then(setReadiness)
      .catch(() =>
        setReadiness({ has_llm_key: false, llm_keys_present: [], llm_keys_missing: [] })
      );
  }, []);

  const llmStatus: "ok" | "warning" | "loading" =
    readiness === null ? "loading" : readiness.has_llm_key ? "ok" : "warning";

  return (
    <div className='space-y-6'>
      {/* Workspace info card */}
      {projectType === "demo" && (
        <div className='space-y-2 rounded-lg border bg-muted/40 p-4'>
          <div className='flex items-center gap-2'>
            <Database className='h-4 w-4 text-primary' />
            <span className='font-medium text-sm'>Demo workspace ready</span>
          </div>
          <p className='text-muted-foreground text-xs'>
            Your demo workspace includes a DuckDB database with sample e-commerce data — orders,
            customers, and products — plus pre-built AI agents ready to query it. No database
            connection setup needed.
          </p>
        </div>
      )}

      {projectType === "new" && (
        <div className='space-y-2 rounded-lg border bg-muted/40 p-4'>
          <div className='flex items-center gap-2'>
            <Database className='h-4 w-4 text-muted-foreground' />
            <span className='font-medium text-sm'>Add a database to get started</span>
          </div>
          <p className='text-muted-foreground text-xs'>
            Your workspace is ready. Go to <span className='font-medium'>Settings → Databases</span>{" "}
            to connect a database and start querying with AI.
          </p>
        </div>
      )}

      {projectType === "github" && (
        <div className='space-y-2 rounded-lg border bg-muted/40 p-4'>
          <div className='flex items-center gap-2'>
            <Database className='h-4 w-4 text-muted-foreground' />
            <span className='font-medium text-sm'>Repository imported</span>
          </div>
          <p className='text-muted-foreground text-xs'>
            Your repository has been cloned. If your workspace has databases configured in{" "}
            <span className='font-medium'>config.yml</span>, they will be available immediately. You
            can also add connections in <span className='font-medium'>Settings → Databases</span>.
          </p>
        </div>
      )}

      {/* LLM key check */}
      <TourCheckItem icon={<Key className='h-4 w-4' />} title='LLM API Keys' status={llmStatus}>
        {readiness === null ? (
          <p className='text-muted-foreground text-xs'>Checking environment…</p>
        ) : (
          <div className='space-y-2'>
            {readiness.llm_keys_present.length > 0 && (
              <div className='space-y-1'>
                <p className='text-muted-foreground text-xs'>Ready:</p>
                <div className='space-y-1 pl-1'>
                  {readiness.llm_keys_present.map((k) => (
                    <LlmKeyRow key={k} name={k} present={true} />
                  ))}
                </div>
              </div>
            )}
            {readiness.llm_keys_missing.length > 0 && (
              <div className='space-y-1'>
                <p className='text-muted-foreground text-xs'>
                  {readiness.llm_keys_present.length > 0 ? "Not configured:" : "None configured:"}
                </p>
                <div className='space-y-1 pl-1'>
                  {readiness.llm_keys_missing.map((k) => (
                    <LlmKeyRow key={k} name={k} present={false} />
                  ))}
                </div>
              </div>
            )}
            {!readiness.has_llm_key && (
              <p className='text-amber-600 text-xs dark:text-amber-400'>
                Set at least one key in your environment or in{" "}
                <span className='font-medium'>Settings → Secrets</span> to enable AI queries.
              </p>
            )}
          </div>
        )}
      </TourCheckItem>

      <Button onClick={onEnterApp} className='w-full'>
        Enter App
      </Button>
    </div>
  );
}
