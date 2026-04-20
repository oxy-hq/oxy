interface Args {
  remoteUrl?: string | null;
  branch: string;
  defaultBranch: string;
  isOnMain: boolean;
}

/**
 * Derive `repo` and `pr` URLs from a GitHub remote URL.
 * Returns `{ repoUrl: null, prUrl: null }` when the remote isn't a GitHub URL
 * or when no remote is configured. The PR URL is null on the default branch
 * (no compare target).
 */
export function useGithubUrls({ remoteUrl, branch, defaultBranch, isOnMain }: Args) {
  const base = (() => {
    if (!remoteUrl) return null;
    const match = remoteUrl.match(/github\.com[/:]([^/]+\/[^/.]+?)(?:\.git)?$/);
    return match ? `https://github.com/${match[1]}` : null;
  })();

  if (!base) return { repoUrl: null, prUrl: null };
  return {
    repoUrl: `${base}/tree/${branch}`,
    prUrl: isOnMain ? null : `${base}/compare/${defaultBranch}...${branch}?expand=1`
  };
}
