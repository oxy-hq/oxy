import { FolderOpen, KeyRound, Loader2 } from "lucide-react";
import { useState } from "react";
import GithubIcon from "@/components/ui/GithubIcon";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { OnboardingService } from "@/services/api/onboarding";

interface Props {
  onBack: () => void;
  onDone: (workspaceId: string) => void;
  workspaceName?: string;
}

export const GitHubUrlOnboardingStep = ({ onBack, onDone, workspaceName }: Props) => {
  const [gitUrl, setGitUrl] = useState("");
  const [branch, setBranch] = useState("");
  const [subdir, setSubdir] = useState("");
  const [localName, setLocalName] = useState(workspaceName ?? "");
  const [token, setToken] = useState("");
  const [needsToken, setNeedsToken] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    if (!gitUrl.trim()) return;
    setIsSubmitting(true);
    setError(null);

    try {
      const result = await OnboardingService.setupGithubUrl({
        git_url: gitUrl.trim(),
        branch: branch.trim() || undefined,
        name: localName.trim() || undefined,
        subdir: subdir.trim() || undefined,
        token: token.trim() || undefined
      });
      onDone(result.workspace_id);
    } catch (err: unknown) {
      // 401 → host credentials failed, ask for a PAT
      const status =
        err && typeof err === "object" && "response" in err
          ? (err as { response?: { status?: number } }).response?.status
          : undefined;
      if (status === 401) {
        setNeedsToken(true);
        setError(
          "Could not access the repository with system credentials. Enter a personal access token to continue."
        );
      } else {
        const body = (err as { response?: { data?: unknown } })?.response?.data;
        setError(
          typeof body === "string" && body.length > 0
            ? body
            : status === 409
              ? "A workspace with that name already exists. Please choose a different name."
              : "Failed to import repository"
        );
      }
      setIsSubmitting(false);
    }
  };

  const canSubmit = !!gitUrl.trim() && !isSubmitting && (!needsToken || !!token.trim());

  return (
    <div className='flex flex-col gap-5'>
      {/* URL */}
      <div className='space-y-2'>
        <Label htmlFor='git-url' className='flex items-center gap-1.5'>
          <GithubIcon className='h-3.5 w-3.5 text-muted-foreground' />
          Repository URL
        </Label>
        <Input
          id='git-url'
          placeholder='https://github.com/your-org/your-repo.git'
          value={gitUrl}
          onChange={(e) => setGitUrl(e.target.value)}
          className='font-mono text-sm'
          autoFocus
        />
        <p className='text-muted-foreground text-xs'>
          HTTPS or SSH URL. System git credentials are tried first.
        </p>
      </div>

      {/* Branch — optional */}
      <div className='space-y-2'>
        <div className='flex items-center gap-1.5'>
          <Label htmlFor='git-branch'>Branch</Label>
          <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground'>
            optional
          </span>
        </div>
        <Input
          id='git-branch'
          placeholder='main'
          value={branch}
          onChange={(e) => setBranch(e.target.value)}
          className='font-mono text-sm'
        />
        <p className='text-muted-foreground text-xs'>
          Leave blank to use the repository default branch.
        </p>
      </div>

      {/* Subdirectory — optional */}
      <div className='space-y-2'>
        <div className='flex items-center gap-1.5'>
          <Label htmlFor='git-subdir'>Subdirectory</Label>
          <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground'>
            optional
          </span>
        </div>
        <div className='relative'>
          <FolderOpen className='pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/50' />
          <Input
            id='git-subdir'
            placeholder='e.g. analytics/oxy'
            value={subdir}
            onChange={(e) => setSubdir(e.target.value)}
            className='pl-8 font-mono text-sm'
          />
        </div>
        <p className='text-muted-foreground text-xs'>
          For monorepos — path to the Oxy workspace folder inside the repo.
        </p>
      </div>

      {/* Workspace name — optional */}
      <div className='space-y-2'>
        <div className='flex items-center gap-1.5'>
          <Label htmlFor='git-name'>Workspace name</Label>
          <span className='rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground'>
            optional
          </span>
        </div>
        <Input
          id='git-name'
          placeholder='my-workspace'
          value={localName}
          onChange={(e) => setLocalName(e.target.value)}
          className='font-mono text-sm'
        />
        <p className='text-muted-foreground text-xs'>Leave blank to use the repository name.</p>
      </div>

      {/* PAT — shown only after a 401 */}
      {needsToken && (
        <div className='space-y-2 rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-800/40 dark:bg-amber-950/20'>
          <Label
            htmlFor='git-token'
            className='flex items-center gap-1.5 text-amber-700 dark:text-amber-400'
          >
            <KeyRound className='h-3.5 w-3.5' />
            Personal access token required
          </Label>
          <Input
            id='git-token'
            type='password'
            placeholder='ghp_••••••••••••••••'
            value={token}
            onChange={(e) => setToken(e.target.value)}
            className='font-mono text-sm'
            autoFocus
          />
          <p className='text-amber-700/80 text-xs dark:text-amber-400/80'>
            Create a token with <code className='font-mono'>repo</code> scope at{" "}
            <span className='font-medium'>
              GitHub → Settings → Developer settings → Personal access tokens
            </span>
            .
          </p>
        </div>
      )}

      {error && !needsToken && <p className='text-destructive text-sm'>{error}</p>}

      <div className='flex gap-3'>
        <Button variant='outline' onClick={onBack} disabled={isSubmitting}>
          Back
        </Button>
        <Button onClick={handleSubmit} disabled={!canSubmit} className='flex-1'>
          {isSubmitting && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
          {needsToken ? "Import with token" : "Import Repository"}
        </Button>
      </div>
    </div>
  );
};
