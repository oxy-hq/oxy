/**
 * IDE editor for `.airway.yml` pipeline files.
 *
 * Mirrors `AutomationEditor`'s split layout but minimal: a `.airway.yml`
 * is plain YAML (no visual form / diagram), so the left pane is the
 * default Monaco editor and the right pane embeds the pipeline / run
 * pair as master-detail. Clicking Run opens the run inline rather
 * than navigating away — the standalone airway URL doesn't exist
 * inside the IDE shell, so the previous fallback `navigate("runs/...",
 * { relative: "path" })` landed on an unmatched route and blanked
 * the right pane.
 */

import { ExternalLink } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { decodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import { AirwayPipelinePage, AirwayRunDetailPage } from "@/pages/airway";
import useCurrentOrg from "@/stores/useCurrentOrg";

import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";

const AirwayEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const { refreshPreview, previewKey } = usePreviewRefresh();
  const [runId, setRunId] = useState<string | null>(null);
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  // Cross-link to the Coordinator's Runs surface filtered to airway
  // runs, pre-searched by this pipeline's name. The user's mental
  // model is "I edit YAML here, I operate the pipeline in the
  // Coordinator" — this link makes the second half of that
  // discoverable without the user having to switch contexts manually.
  const pipelineName = useMemo(() => {
    const path = decodeBase64(pathb64);
    const base = path.split("/").pop() ?? path;
    return base.replace(/\.airway\.(yml|yaml)$/, "");
  }, [pathb64]);

  const coordinatorRunsHref = `${ROUTES.ORG(orgSlug).WORKSPACE(project.id).IDE.COORDINATOR.RUNS}?source=airway&search=${encodeURIComponent(pipelineName)}`;

  const preview = runId ? (
    <AirwayRunDetailPage
      key={`${pathb64}:${runId}:${previewKey}`}
      pathb64={pathb64}
      runId={runId}
      hideHeader
      onBack={() => setRunId(null)}
      onOpenRun={setRunId}
    />
  ) : (
    <AirwayPipelinePage
      key={`${pathb64}:${previewKey}`}
      pathb64={pathb64}
      hideHeader
      onOpenRun={setRunId}
    />
  );

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={refreshPreview}
      git={gitEnabled}
      preview={
        <div className='flex h-full flex-col'>
          <div className='flex items-center justify-end gap-2 border-border border-b px-3 py-1.5'>
            <Button asChild variant='ghost' size='sm' className='h-7'>
              <Link to={coordinatorRunsHref} data-testid='airway-editor-view-coordinator-runs'>
                <ExternalLink className='h-3.5 w-3.5' />
                View runs in Coordinator
              </Link>
            </Button>
          </div>
          <div className='min-h-0 flex-1'>{preview}</div>
        </div>
      }
    />
  );
};

export default AirwayEditor;
