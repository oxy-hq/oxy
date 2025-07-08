export interface GitHubScope {
  name: string;
  description: string;
}

export const GITHUB_SCOPES: GitHubScope[] = [
  { name: "repo", description: "Access to repositories" },
  { name: "user:email", description: "Access to user email addresses" },
  { name: "read:user", description: "Access to user profile information" },
];

export const GITHUB_TOKEN_URL =
  "https://github.com/settings/tokens/new?scopes=repo,user:email,read:user&description=Oxy%20Integration";

export const NAVIGATION_DELAY = 1500;
