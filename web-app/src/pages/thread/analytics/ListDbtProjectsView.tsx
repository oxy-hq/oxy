import { FolderOpen } from "lucide-react";
import ErrorAlert from "@/components/ui/ErrorAlert";
import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { TimingBar } from "./AnalyticsArtifactViews";
import { parseToolJson } from "./analyticsArtifactHelpers";

interface DbtProjectInfo {
  name?: string;
  project_dir?: string;
  model_paths?: string[];
  seed_paths?: string[];
}

const ListDbtProjectsView = ({ item }: { item: ArtifactItem }) => {
  const output = parseToolJson<{
    ok?: boolean;
    projects?: DbtProjectInfo[];
    error?: string;
  }>(item.toolOutput);
  const projects = output?.projects ?? [];

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-auto p-4'>
        <div className='space-y-4'>
          <div className='rounded border bg-muted/30 px-2.5 py-2'>
            <p className='text-[10px] text-muted-foreground uppercase tracking-wide'>Projects</p>
            <p className='font-medium font-mono text-xs'>{projects.length}</p>
          </div>

          {output?.error && <ErrorAlert message={output.error} />}

          {projects.length > 0 && (
            <div className='space-y-2'>
              {projects.map((proj) => (
                <div
                  key={proj.name ?? proj.project_dir}
                  className='rounded border bg-muted/30 px-3 py-2.5'
                >
                  <div className='flex items-center gap-2'>
                    <FolderOpen className='h-3.5 w-3.5 shrink-0 text-primary' />
                    <span className='font-medium font-mono text-xs'>{proj.name ?? "—"}</span>
                  </div>
                  {proj.project_dir && (
                    <p className='mt-1 break-all font-mono text-[10px] text-muted-foreground'>
                      {proj.project_dir}
                    </p>
                  )}
                  {proj.model_paths && proj.model_paths.length > 0 && (
                    <p className='mt-1 text-[10px] text-muted-foreground'>
                      <span className='text-muted-foreground/60'>models: </span>
                      {proj.model_paths.join(", ")}
                    </p>
                  )}
                  {proj.seed_paths && proj.seed_paths.length > 0 && (
                    <p className='mt-0.5 text-[10px] text-muted-foreground'>
                      <span className='text-muted-foreground/60'>seeds: </span>
                      {proj.seed_paths.join(", ")}
                    </p>
                  )}
                </div>
              ))}
            </div>
          )}

          {projects.length === 0 && !output?.error && (
            <p className='text-muted-foreground text-xs'>No dbt projects found.</p>
          )}
        </div>
      </div>
      <TimingBar item={item} />
    </div>
  );
};

export default ListDbtProjectsView;
