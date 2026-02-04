import { useCallback, useState } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { DatabaseService } from "@/services/api";
import type {
  ConnectionTestEvent,
  TestDatabaseConnectionRequest,
  TestDatabaseConnectionResponse
} from "@/types/database";

export interface TestConnectionState {
  isLoading: boolean;
  progress: string[];
  ssoUrl: string | null;
  ssoMessage: string | null;
  ssoTimeout: number | null;
  result: TestDatabaseConnectionResponse | null;
  error: string | null;
}

export function useTestDatabaseConnection() {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  const [state, setState] = useState<TestConnectionState>({
    isLoading: false,
    progress: [],
    ssoUrl: null,
    ssoMessage: null,
    ssoTimeout: null,
    result: null,
    error: null
  });

  const testConnection = useCallback(
    async (request: TestDatabaseConnectionRequest) => {
      // Reset state
      setState({
        isLoading: true,
        progress: [],
        ssoUrl: null,
        ssoMessage: null,
        ssoTimeout: null,
        result: null,
        error: null
      });

      try {
        await DatabaseService.testDatabaseConnection(
          projectId,
          branchName,
          request,
          (event: ConnectionTestEvent) => {
            console.log("Connection Test Event:", event);
            switch (event.type) {
              case "progress":
                setState((prev) => ({
                  ...prev,
                  progress: [...prev.progress, event.message]
                }));
                break;

              case "browser_auth_required":
                setState((prev) => ({
                  ...prev,
                  ssoUrl: event.sso_url,
                  ssoMessage: event.message,
                  ssoTimeout: event.timeout_secs || null,
                  progress: [...prev.progress, event.message]
                }));
                break;

              case "complete":
                setState((prev) => ({
                  ...prev,
                  isLoading: false,
                  result: event.result,
                  error: event.result.success
                    ? null
                    : event.result.error_details || event.result.message
                }));
                break;
            }
          }
        );
      } catch (error) {
        setState((prev) => ({
          ...prev,
          isLoading: false,
          error: error instanceof Error ? error.message : "Failed to test connection"
        }));
      }
    },
    [projectId, branchName]
  );

  const reset = useCallback(() => {
    setState({
      isLoading: false,
      progress: [],
      ssoUrl: null,
      ssoMessage: null,
      ssoTimeout: null,
      result: null,
      error: null
    });
  }, []);

  return {
    ...state,
    testConnection,
    reset
  };
}
