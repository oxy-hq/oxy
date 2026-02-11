import { apiClient } from "./axios";

export interface BuildInfo {
  git_commit: string;
  git_commit_short: string;
  build_timestamp: string;
  build_profile: string;
  commit_url?: string;
  workflow_url?: string;
}

export interface VersionInfo {
  version: string;
  service: string;
  build_info: BuildInfo;
}

export async function getVersion(): Promise<VersionInfo> {
  const response = await apiClient.get<{
    version: string;
    service: string;
    build_info: BuildInfo;
  }>("/version");
  return {
    version: response.data.version,
    service: response.data.service,
    build_info: response.data.build_info
  };
}
