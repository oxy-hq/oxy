import { apiClient } from "./axios";

interface BuildInfo {
  git_commit: string;
  git_commit_short: string;
  build_timestamp: string;
  build_profile: string;
  commit_url?: string;
  workflow_url?: string;
  sentry_release?: string;
}

export interface VersionInfo {
  /** The image tag the server runs as (`main-<sha>`); null outside an image. */
  image_tag: string | null;
  service: string;
  build_info: BuildInfo;
}

export async function getVersion(): Promise<VersionInfo> {
  const response = await apiClient.get<VersionInfo>("/version");
  return {
    image_tag: response.data.image_tag ?? null,
    service: response.data.service,
    build_info: response.data.build_info
  };
}

/**
 * One word for the running build: its image tag, or its commit when the server is not an
 * image (a local build reports `dev`). Never the semver, which stays the same for every
 * image between two releases.
 */
export const buildLabel = (info: VersionInfo): string =>
  info.image_tag ?? info.build_info.git_commit_short;
