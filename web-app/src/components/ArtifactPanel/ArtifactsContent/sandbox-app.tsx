import { SandboxAppArtifact } from "@/types/artifact";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/shadcn/alert";
import {
  Loader2,
  ExternalLink,
  Maximize,
  Minimize,
  RotateCw,
  Terminal,
  X,
} from "lucide-react";
import { useState, useEffect, useRef } from "react";
import useCurrentProject from "@/stores/useCurrentProject";
import { apiBaseURL } from "@/services/env";

type Props = {
  artifact: SandboxAppArtifact;
  /**
   * Optional API key to provide to iframe via postMessage.
   * If not provided, will attempt to read from localStorage 'oxy_api_key'
   */
  apiKey?: string;
};

interface V0LogMessage {
  currentURL: string;
  error: string;
  isFatal?: boolean;
  isServer: boolean;
  stack: string;
  type: "error";
  __v0_remote__: number;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const isV0LogMessage = (obj: any): obj is V0LogMessage => {
  return obj && obj.type === "error" && typeof obj.__v0_remote__ === "number";
};

const v0ToConsoleLog = (v0Log: V0LogMessage): ConsoleLog => {
  const levelMap: { [key: string]: ConsoleLog["level"] } = {
    debug: "log",
    info: "info",
    warn: "warn",
    error: "error",
  };

  return {
    // eslint-disable-next-line sonarjs/pseudo-random
    id: `${Date.now()}-${Math.random()}`,
    timestamp: Date.now(),
    level: levelMap[v0Log.type] || "log",
    args: [v0Log.error, v0Log.stack, `(isServer: ${v0Log.isServer})`],
  };
};

type ConsoleLog = {
  id: string;
  timestamp: number;
  level: "log" | "info" | "warn" | "error";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  args: any[];
};

const SandboxArtifactPanel = ({ artifact, apiKey }: Props) => {
  const [isLoading, setIsLoading] = useState(true);
  const [hasError, setHasError] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showConsole, setShowConsole] = useState(false);
  const [consoleLogs, setConsoleLogs] = useState<ConsoleLog[]>([]);
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const consoleEndRef = useRef<HTMLDivElement>(null);

  const { preview_url } = artifact.content.value;
  const projectId = useCurrentProject((state) => state.project?.id);

  const handleLoad = () => setIsLoading(false);
  const handleError = () => {
    setIsLoading(false);
    setHasError(true);
  };

  const handleRefresh = () => {
    if (iframeRef.current) {
      setIsLoading(true);
      iframeRef.current.src = preview_url;
    }
  };

  const handleFullscreen = () => {
    if (!containerRef.current) return;

    if (!isFullscreen) {
      if (containerRef.current.requestFullscreen) {
        containerRef.current.requestFullscreen();
      }
    } else {
      if (document.exitFullscreen) {
        document.exitFullscreen();
      }
    }
  };

  // Set up postMessage listener for SDK authentication requests
  useEffect(() => {
    const handleAuthRequest = (event: MessageEvent) => {
      // Check if this is an Oxy SDK auth request
      if (!event.data || event.data.type !== "OXY_AUTH_REQUEST") {
        return;
      }

      // Verify the request is from our iframe
      if (
        !iframeRef.current ||
        event.source !== iframeRef.current.contentWindow
      ) {
        console.warn("[Oxy] Auth request from unknown source, ignoring");
        return;
      }

      // Validate iframe origin matches preview_url origin
      try {
        const previewOrigin = new URL(preview_url).origin;
        if (event.origin !== previewOrigin) {
          console.warn(
            "[Oxy] Origin mismatch:",
            event.origin,
            "vs",
            previewOrigin,
          );
          return;
        }
      } catch {
        console.error("[Oxy] Invalid preview URL:", preview_url);
        return;
      }

      // Get API key from props or localStorage
      const userApiKey = apiKey || localStorage.getItem("auth_token");

      if (!userApiKey) {
        console.error("[Oxy] No API key available to provide to iframe");
      }

      console.log("[Oxy] Sending API key to iframe");

      // Send authentication response to iframe
      if (event.source) {
        event.source.postMessage(
          {
            type: "OXY_AUTH_RESPONSE",
            version: "1.0",
            requestId: event.data.requestId,
            apiKey: userApiKey,
            projectId: projectId,
            baseUrl: apiBaseURL,
          },
          event.origin,
        );
      }
    };

    // Add event listener
    window.addEventListener("message", handleAuthRequest);

    // Cleanup on unmount
    return () => {
      window.removeEventListener("message", handleAuthRequest);
    };
  }, [preview_url, apiKey, projectId]);

  // Set up postMessage listener for console logs from iframe
  useEffect(() => {
    const handleConsoleLog = (event: MessageEvent) => {
      // Check if this is an Oxy console log message
      const data = event.data;
      if (!isV0LogMessage(data)) {
        return;
      }
      console.log("[Oxy] Received console log from iframe:", data);

      // Verify the message is from our iframe
      if (
        !iframeRef.current ||
        event.source !== iframeRef.current.contentWindow
      ) {
        return;
      }

      // Validate iframe origin matches preview_url origin
      try {
        const previewOrigin = new URL(preview_url).origin;
        if (event.origin !== previewOrigin) {
          return;
        }
      } catch {
        return;
      }

      // Add the log to our console logs
      const log: ConsoleLog = v0ToConsoleLog(data);

      setConsoleLogs((prev) => [...prev, log]);
    };

    window.addEventListener("message", handleConsoleLog);

    return () => {
      window.removeEventListener("message", handleConsoleLog);
    };
  }, [preview_url]);

  // Auto-scroll console to bottom when new logs arrive
  useEffect(() => {
    if (showConsole && consoleEndRef.current) {
      consoleEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [consoleLogs, showConsole]);

  // Listen for fullscreen changes
  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };

    document.addEventListener("fullscreenchange", handleFullscreenChange);
    return () => {
      document.removeEventListener("fullscreenchange", handleFullscreenChange);
    };
  }, []);

  // Validate preview_url
  if (!preview_url) {
    return artifact.is_streaming ? (
      <div className="p-4">
        <Alert>
          {/* Streaming state with loader */}
          <AlertTitle>
            <div className="flex items-center gap-2 mb-2">
              <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
              Generating Preview...
            </div>
          </AlertTitle>
          <AlertDescription>
            The sandbox preview is being generated. Please wait a moment.
          </AlertDescription>
        </Alert>
      </div>
    ) : (
      <div className="p-4">
        <Alert variant="destructive">
          <AlertTitle>Invalid Sandbox</AlertTitle>
          <AlertDescription>
            No preview URL available for this sandbox app.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex flex-col h-full relative">
      {/* Loading overlay */}
      {isLoading && (
        <div className="absolute inset-0 flex items-center justify-center bg-background/50 z-10">
          <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
        </div>
      )}

      {/* Error state */}
      {hasError && (
        <div className="p-4">
          <Alert variant="destructive">
            <AlertTitle>Failed to Load Preview</AlertTitle>
            <AlertDescription>
              Unable to load the sandbox preview. The preview may have expired
              or is unavailable.
              <a
                href={preview_url}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-1 mt-2 text-primary hover:underline"
              >
                Open in new tab <ExternalLink className="w-3 h-3" />
              </a>
            </AlertDescription>
          </Alert>
        </div>
      )}

      {/* Iframe */}
      {!hasError && (
        <iframe
          ref={iframeRef}
          src={preview_url}
          className="w-full h-full border-0"
          title="Sandbox Preview"
          onLoad={handleLoad}
          onError={handleError}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox"
          allow="accelerometer; camera; encrypted-media; geolocation; gyroscope; microphone"
        />
      )}

      {/* Omni bar with controls */}
      <div className="absolute top-2 right-2 z-20 flex gap-1 bg-background/80 backdrop-blur rounded-md border border-base-border p-1">
        {artifact.is_streaming && (
          <div className="flex items-center gap-2 px-2 text-sm text-muted-foreground">
            <Loader2 className="w-4 h-4 animate-spin" />
            <span>Generating...</span>
          </div>
        )}
        <button
          onClick={handleRefresh}
          className="p-2 hover:bg-muted rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          title="Refresh page"
          disabled={artifact.is_streaming}
        >
          <RotateCw className="w-4 h-4" />
        </button>
        <button
          onClick={() => setShowConsole(!showConsole)}
          className={`p-2 hover:bg-muted rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
            showConsole ? "bg-muted" : ""
          }`}
          title={showConsole ? "Hide console" : "Show console"}
          disabled={artifact.is_streaming}
        >
          <Terminal className="w-4 h-4" />
        </button>
        <button
          onClick={handleFullscreen}
          className="p-2 hover:bg-muted rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          title={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
          disabled={artifact.is_streaming}
        >
          {isFullscreen ? (
            <Minimize className="w-4 h-4" />
          ) : (
            <Maximize className="w-4 h-4" />
          )}
        </button>
        <a
          href={preview_url}
          target="_blank"
          rel="noopener noreferrer"
          className={`p-2 hover:bg-muted rounded transition-colors flex ${
            artifact.is_streaming
              ? "opacity-50 pointer-events-none cursor-not-allowed"
              : ""
          }`}
          title="Open in new tab"
        >
          <ExternalLink className="w-4 h-4" />
        </a>
      </div>

      {/* Console Panel */}
      {showConsole && (
        <div className="absolute bottom-0 left-0 right-0 z-20 bg-background border-t border-base-border h-64 flex flex-col">
          {/* Console Header */}
          <div className="flex items-center justify-between px-4 py-2 border-b border-base-border bg-muted/30">
            <div className="flex items-center gap-2">
              <Terminal className="w-4 h-4" />
              <span className="font-semibold text-sm">Console</span>
              <span className="text-xs text-muted-foreground">
                ({consoleLogs.length}{" "}
                {consoleLogs.length === 1 ? "log" : "logs"})
              </span>
            </div>
            <div className="flex items-center gap-1">
              <button
                onClick={() => setConsoleLogs([])}
                className="px-2 py-1 text-xs hover:bg-muted rounded transition-colors"
                title="Clear console"
              >
                Clear
              </button>
              <button
                onClick={() => setShowConsole(false)}
                className="p-1 hover:bg-muted rounded transition-colors"
                title="Close console"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Console Logs */}
          <div className="flex-1 overflow-y-auto font-mono text-xs p-2 space-y-1">
            {consoleLogs.length === 0 ? (
              <div className="text-muted-foreground text-center py-4">
                No console logs yet
              </div>
            ) : (
              consoleLogs.map((log) => {
                const levelColors = {
                  log: "text-foreground",
                  info: "text-blue-500",
                  warn: "text-yellow-500",
                  error: "text-red-500",
                };

                const levelBgColors = {
                  log: "bg-muted/30",
                  info: "bg-blue-500/10",
                  warn: "bg-yellow-500/10",
                  error: "bg-red-500/10",
                };

                return (
                  <div
                    key={log.id}
                    className={`px-2 py-1 rounded ${levelBgColors[log.level]}`}
                  >
                    <div className="flex items-start gap-2">
                      <span className="text-muted-foreground text-[10px] whitespace-nowrap">
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </span>
                      <span
                        className={`font-semibold uppercase text-[10px] ${levelColors[log.level]}`}
                      >
                        {log.level}
                      </span>
                      <span className={`flex-1 ${levelColors[log.level]}`}>
                        {log.args
                          .map((arg) =>
                            typeof arg === "object"
                              ? JSON.stringify(arg, null, 2)
                              : String(arg),
                          )
                          .join(" ")}
                      </span>
                    </div>
                  </div>
                );
              })
            )}
            <div ref={consoleEndRef} />
          </div>
        </div>
      )}
    </div>
  );
};

export default SandboxArtifactPanel;
