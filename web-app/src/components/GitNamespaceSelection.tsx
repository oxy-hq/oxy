import {
  AlertTriangle,
  BookOpen,
  Building2,
  Check,
  Github,
  Key,
  Loader2,
  Plus,
  User,
  X
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import {
  useConnectNamespaceFromOAuth,
  useCreateGitNamespace,
  useCreateInstallationNamespace,
  useCreatePATNamespace,
  useDeleteGitNamespace,
  useGitHubAppInstallations,
  useGitHubInstallAppUrl,
  useGitHubNamespaces,
  usePickNamespaceInstallation
} from "@/hooks/api/github";
import { GITHUB_OAUTH_CALLBACK_MESSAGE } from "@/pages/github/callback";
import { GitHubApiService } from "@/services/api";
import type { OAuthInstallation } from "@/types/github";
import { openGitHubAppInstallation } from "@/utils/githubAppInstall";

// ─── GitHub App setup guide ───────────────────────────────────────────────────

const REQUIRED_PERMISSIONS = [
  { scope: "Contents", access: "Read & Write", reason: "clone repos and push commits" },
  { scope: "Metadata", access: "Read", reason: "required by the GitHub API" },
  { scope: "Administration", access: "Read & Write", reason: "create repositories" }
];

const REQUIRED_ENV_VARS = [
  { name: "GITHUB_APP_ID", desc: "Numeric App ID from the app's settings page" },
  { name: "GITHUB_APP_SLUG", desc: "URL slug (e.g. my-oxy-app)" },
  { name: "GITHUB_APP_PRIVATE_KEY", desc: "PEM private key generated in app settings" },
  { name: "GITHUB_CLIENT_ID", desc: "OAuth client ID from app settings" },
  { name: "GITHUB_CLIENT_SECRET", desc: "OAuth client secret from app settings" },
  { name: "GITHUB_STATE_SECRET", desc: "Any random string used to sign OAuth state tokens" }
];

function GitHubAppSetupGuide() {
  return (
    <div className='space-y-4 text-sm'>
      <div className='flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2.5 dark:border-amber-800/40 dark:bg-amber-950/20'>
        <BookOpen className='mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-600 dark:text-amber-400' />
        <p className='text-amber-700 text-xs leading-snug dark:text-amber-400'>
          No GitHub App configured. Ask an admin to complete the setup below.
        </p>
      </div>

      <div className='space-y-1.5'>
        <p className='font-medium text-xs'>1. Create a GitHub App</p>
        <p className='text-muted-foreground text-xs leading-relaxed'>
          Go to{" "}
          <span className='rounded bg-muted px-1 font-mono text-[11px]'>
            GitHub → Settings → Developer settings → GitHub Apps → New GitHub App
          </span>
          . Set the callback URL to{" "}
          <span className='rounded bg-muted px-1 font-mono text-[11px]'>
            {window.location.origin}/github/callback
          </span>
          .
        </p>
      </div>

      <div className='space-y-1.5'>
        <p className='font-medium text-xs'>2. Grant repository permissions</p>
        <div className='overflow-hidden rounded-md border border-border/60'>
          {REQUIRED_PERMISSIONS.map((p, i) => (
            <div
              key={p.scope}
              className={`flex items-center gap-2 px-3 py-1.5 ${i > 0 ? "border-border/40 border-t" : ""}`}
            >
              <Check className='h-3 w-3 shrink-0 text-primary' />
              <span className='w-28 font-mono text-[11px] text-foreground'>{p.scope}</span>
              <span className='w-24 text-[11px] text-muted-foreground'>{p.access}</span>
              <span className='text-[11px] text-muted-foreground/60'>{p.reason}</span>
            </div>
          ))}
        </div>
      </div>

      <div className='space-y-1.5'>
        <p className='font-medium text-xs'>3. Set environment variables</p>
        <div className='overflow-hidden rounded-md border border-border/60'>
          {REQUIRED_ENV_VARS.map((v, i) => (
            <div
              key={v.name}
              className={`flex items-start gap-2 px-3 py-1.5 ${i > 0 ? "border-border/40 border-t" : ""}`}
            >
              <span className='w-52 shrink-0 font-mono text-[11px] text-foreground'>{v.name}</span>
              <span className='text-[11px] text-muted-foreground/70'>{v.desc}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ─── Types ────────────────────────────────────────────────────────────────────

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

function ConnectDialog({
  open,
  onClose,
  onConnected
}: {
  open: boolean;
  onClose: () => void;
  onConnected: (namespaceId: string) => void;
}) {
  const [patToken, setPATToken] = useState("");
  const [patError, setPATError] = useState<string | null>(null);
  // OAuth popup (primary flow: sign in → check installation)
  const [oauthPopupOpened, setOauthPopupOpened] = useState(false);
  const [oauthInstallations, setOauthInstallations] = useState<OAuthInstallation[] | null>(null);
  const [oauthError, setOauthError] = useState<string | null>(null);
  // Shown after OAuth confirms app is not installed
  const [needsInstall, setNeedsInstall] = useState(false);
  // App install popup (only shown as follow-up when app not installed)
  const [installPopupOpened, setInstallPopupOpened] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);
  const oauthPopupRef = useRef<Window | null>(null);
  const installPopupRef = useRef<Window | null>(null);

  const {
    data: installAppUrl,
    isLoading: isLoadingAppUrl,
    isError: appNotConfigured
  } = useGitHubInstallAppUrl();
  const { mutate: createPAT, isPending: isPATLoading } = useCreatePATNamespace();
  const { mutate: createGitNamespace, isPending: isInstallConnecting } = useCreateGitNamespace();
  const { mutate: connectFromOAuth, isPending: isOAuthConnecting } = useConnectNamespaceFromOAuth();
  const { mutate: pickInstallation, isPending: isPickingInstallation } =
    usePickNamespaceInstallation();

  // Unified postMessage listener.
  // Install callback has installation_id; OAuth callback does not.
  useEffect(() => {
    if (!oauthPopupOpened && !installPopupOpened) return;

    // Accept messages from the same origin, and optionally from a configured
    // GitHub callback origin (set VITE_GITHUB_CALLBACK_ORIGIN for cross-domain
    // relay when a shared GitHub App on a different domain handles the callback).
    const allowedOrigin = import.meta.env.VITE_GITHUB_CALLBACK_ORIGIN || window.location.origin;
    const handleMessage = (e: MessageEvent) => {
      if (e.origin !== window.location.origin && e.origin !== allowedOrigin) return;
      if (e.data?.type !== GITHUB_OAUTH_CALLBACK_MESSAGE) return;
      const { installation_id, code, state } = e.data as {
        type: string;
        installation_id?: string;
        code: string;
        state: string;
      };

      if (installation_id) {
        // Install flow completed
        setInstallPopupOpened(false);
        installPopupRef.current = null;
        setInstallError(null);
        setNeedsInstall(false);
        createGitNamespace(
          { installation_id, code, state },
          {
            onSuccess: (ns) => onConnected(ns.id),
            onError: (err) => setInstallError(err.message || "Failed to connect.")
          }
        );
      } else if (code && state) {
        // OAuth sign-in completed — check installation status
        setOauthPopupOpened(false);
        oauthPopupRef.current = null;
        setOauthError(null);
        connectFromOAuth(
          { code, state },
          {
            onSuccess: (result) => {
              if (result.status === "connected") {
                onConnected(result.namespace.id);
              } else if (result.status === "choose") {
                setOauthInstallations(result.installations);
              } else {
                // App not installed — surface install step
                setNeedsInstall(true);
              }
            },
            onError: (err) => setOauthError(err.message || "Authentication failed.")
          }
        );
      }
    };

    window.addEventListener("message", handleMessage);
    const closedCheck = setInterval(() => {
      if (oauthPopupOpened && oauthPopupRef.current?.closed) {
        setOauthPopupOpened(false);
        oauthPopupRef.current = null;
      }
      if (installPopupOpened && installPopupRef.current?.closed) {
        setInstallPopupOpened(false);
        installPopupRef.current = null;
      }
    }, 500);
    return () => {
      window.removeEventListener("message", handleMessage);
      clearInterval(closedCheck);
    };
  }, [oauthPopupOpened, installPopupOpened, createGitNamespace, connectFromOAuth, onConnected]);

  const handlePATSubmit = () => {
    if (!patToken.trim()) return;
    setPATError(null);
    createPAT(patToken.trim(), {
      onSuccess: (ns) => {
        setPATToken("");
        onConnected(ns.id);
      },
      onError: (e) => {
        setPATError(e.message || "Invalid token — needs 'repo' scope.");
      }
    });
  };

  const handleSignIn = async () => {
    setOauthError(null);
    setOauthInstallations(null);
    setNeedsInstall(false);
    try {
      const url = await GitHubApiService.getOAuthConnectUrl();
      const popup = window.open(url, "_blank", "width=600,height=700,noopener=no");
      if (popup) {
        oauthPopupRef.current = popup;
        setOauthPopupOpened(true);
      }
    } catch {
      setOauthError("Couldn't open GitHub. Please try again.");
    }
  };

  const handleInstallApp = async () => {
    if (!installAppUrl) return;
    setInstallError(null);
    const popup = await openGitHubAppInstallation(installAppUrl);
    if (popup) {
      installPopupRef.current = popup;
      setInstallPopupOpened(true);
    }
  };

  const handlePickInstallation = (installationId: number) => {
    pickInstallation(installationId, {
      onSuccess: (ns) => {
        setOauthInstallations(null);
        onConnected(ns.id);
      },
      onError: (err) => setOauthError(err.message || "Failed to connect.")
    });
  };

  const handleClose = () => {
    setPATToken("");
    setPATError(null);
    setOauthPopupOpened(false);
    setOauthInstallations(null);
    setOauthError(null);
    setNeedsInstall(false);
    setInstallPopupOpened(false);
    setInstallError(null);
    onClose();
  };

  const anyBusy =
    oauthPopupOpened ||
    isOAuthConnecting ||
    installPopupOpened ||
    isInstallConnecting ||
    isPickingInstallation;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>Connect GitHub</DialogTitle>
          <DialogDescription>Link a GitHub account to import repositories.</DialogDescription>
        </DialogHeader>

        <Tabs defaultValue='app'>
          <TabsList className='w-full'>
            <TabsTrigger value='app' className='flex-1 gap-1.5'>
              <Github className='h-3.5 w-3.5' />
              GitHub App
            </TabsTrigger>
            <TabsTrigger value='pat' className='flex-1 gap-1.5'>
              <Key className='h-3.5 w-3.5' />
              Token
            </TabsTrigger>
          </TabsList>

          {/* PAT tab */}
          <TabsContent value='pat' className='mt-4 space-y-4'>
            <div className='flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2.5 dark:border-amber-800/40 dark:bg-amber-950/20'>
              <AlertTriangle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-600 dark:text-amber-400' />
              <p className='text-amber-700 text-xs leading-snug dark:text-amber-400'>
                Recommended for personal use or quick trials only. All commits will appear under the
                token owner's account — for shared workspaces or team use, connect via the GitHub
                App instead.
              </p>
            </div>
            <p className='text-muted-foreground text-sm'>
              Generate a token at{" "}
              <a
                href='https://github.com/settings/tokens/new?scopes=repo&description=Oxy'
                target='_blank'
                rel='noreferrer'
                className='text-primary underline-offset-4 hover:underline'
              >
                github.com/settings/tokens
              </a>{" "}
              with the <code className='rounded bg-muted px-1 text-xs'>repo</code> scope.
            </p>

            <div className='space-y-1.5'>
              <Label htmlFor='pat-input'>Token</Label>
              <Input
                id='pat-input'
                ref={inputRef}
                type='password'
                placeholder='ghp_… or github_pat_…'
                value={patToken}
                onChange={(e) => setPATToken(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handlePATSubmit()}
                autoComplete='off'
              />
              {patError && <p className='text-destructive text-xs'>{patError}</p>}
            </div>

            <Button
              className='w-full'
              onClick={handlePATSubmit}
              disabled={!patToken.trim() || isPATLoading}
            >
              {isPATLoading && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
              Connect
            </Button>
          </TabsContent>

          {/* GitHub App tab */}
          <TabsContent value='app' className='mt-4 space-y-4'>
            {appNotConfigured ? (
              <GitHubAppSetupGuide />
            ) : (
              <>
                {/* Step 1: Sign in — always the first action */}
                {!needsInstall && !oauthInstallations && (
                  <Button
                    className='w-full gap-2'
                    onClick={handleSignIn}
                    disabled={anyBusy || isLoadingAppUrl}
                  >
                    {oauthPopupOpened || isOAuthConnecting ? (
                      <Loader2 className='h-4 w-4 animate-spin' />
                    ) : (
                      <Github className='h-4 w-4' />
                    )}
                    {oauthPopupOpened
                      ? "Waiting for GitHub…"
                      : isOAuthConnecting
                        ? "Connecting…"
                        : "Sign in with GitHub"}
                  </Button>
                )}

                {/* Step 2a: App not installed — offer install */}
                {needsInstall && (
                  <div className='space-y-3'>
                    <div className='rounded-lg border border-border/60 bg-muted/30 px-3 py-3'>
                      <p className='font-medium text-sm'>GitHub App not installed</p>
                      <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                        Install the GitHub App on your account or organization to continue.
                      </p>
                    </div>

                    <Button
                      variant='outline'
                      className='w-full gap-2'
                      onClick={handleInstallApp}
                      disabled={!installAppUrl || isLoadingAppUrl || anyBusy}
                    >
                      {installPopupOpened || isInstallConnecting ? (
                        <Loader2 className='h-4 w-4 animate-spin' />
                      ) : (
                        <Github className='h-4 w-4' />
                      )}
                      {installPopupOpened
                        ? "Waiting for GitHub…"
                        : isInstallConnecting
                          ? "Connecting…"
                          : "Install GitHub App"}
                    </Button>

                    <button
                      type='button'
                      onClick={() => setNeedsInstall(false)}
                      className='w-full text-center text-muted-foreground text-xs hover:text-foreground'
                    >
                      ← Back
                    </button>

                    {installError && <p className='text-destructive text-xs'>{installError}</p>}
                  </div>
                )}

                {/* Step 2b: Multiple installations found — pick one */}
                {oauthInstallations && oauthInstallations.length > 0 && (
                  <div className='space-y-2'>
                    <p className='text-muted-foreground text-xs'>
                      Multiple installations found. Choose one:
                    </p>
                    <div className='flex flex-col gap-1.5'>
                      {oauthInstallations.map((inst) => (
                        <button
                          key={inst.id}
                          type='button'
                          onClick={() => handlePickInstallation(inst.id)}
                          disabled={isPickingInstallation}
                          className='flex items-center gap-3 rounded-lg border border-border px-3 py-2.5 text-left transition-colors hover:border-primary hover:bg-primary/5 disabled:opacity-50'
                        >
                          {inst.owner_type === "Organization" ? (
                            <Building2 className='h-4 w-4 shrink-0 text-muted-foreground' />
                          ) : (
                            <User className='h-4 w-4 shrink-0 text-muted-foreground' />
                          )}
                          <span className='flex-1 font-medium text-sm'>{inst.name}</span>
                          <Badge variant='outline' className='shrink-0 text-xs'>
                            {inst.owner_type}
                          </Badge>
                          {isPickingInstallation && (
                            <Loader2 className='h-3.5 w-3.5 animate-spin text-muted-foreground' />
                          )}
                        </button>
                      ))}
                    </div>

                    <button
                      type='button'
                      onClick={() => setOauthInstallations(null)}
                      className='w-full text-center text-muted-foreground text-xs hover:text-foreground'
                    >
                      ← Back
                    </button>
                  </div>
                )}

                {oauthError && <p className='text-destructive text-xs'>{oauthError}</p>}
              </>
            )}
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

export const GitNamespaceSelection = ({ value, onChange }: Props) => {
  const {
    data: gitNamespaces = [],
    isPending: isLoadingNamespaces,
    refetch
  } = useGitHubNamespaces();
  const { mutate: deleteNamespace } = useDeleteGitNamespace();

  // Track deliberate disconnects so we don't auto-reconnect or re-fetch the
  // global installations list after the user intentionally removes their account.
  const [userDisconnected, setUserDisconnected] = useState(false);

  // Only query the global installations list when the user has no connected
  // namespaces and has not deliberately disconnected. The endpoint uses the
  // App JWT and returns ALL installations across all tenants — it must not
  // be called after an intentional removal.
  const { data: appInstallations = [], isPending: isLoadingInstallations } =
    useGitHubAppInstallations(gitNamespaces.length === 0 && !userDisconnected);
  const { mutate: autoConnect } = useCreateInstallationNamespace();
  const autoConnectAttempted = useRef(false);

  const [connectOpen, setConnectOpen] = useState(false);
  // Snapshot the namespace count when the connect dialog opens so we can
  // detect when a new namespace is added (e.g. via popup on a different domain).
  const namespaceCountOnOpenRef = useRef(0);
  useEffect(() => {
    if (connectOpen) namespaceCountOnOpenRef.current = gitNamespaces.length;
  }, [connectOpen, gitNamespaces.length]);

  // When the GitHub App has exactly one installation and no namespaces are
  // connected yet, silently create the namespace so the user skips the dialog.
  useEffect(() => {
    if (
      autoConnectAttempted.current ||
      isLoadingNamespaces ||
      isLoadingInstallations ||
      gitNamespaces.length > 0 ||
      appInstallations.length !== 1
    ) {
      return;
    }
    autoConnectAttempted.current = true;
    autoConnect(appInstallations[0].id, {
      onSuccess: (ns) => {
        refetch().then(() => onChange?.(ns.id));
      }
    });
  }, [
    appInstallations,
    isLoadingInstallations,
    isLoadingNamespaces,
    gitNamespaces.length,
    autoConnect,
    refetch,
    onChange
  ]);

  // When a new namespace appears while the connect dialog is open, treat it as
  // a successful connection and close the dialog automatically. This replaces
  // the localStorage-based approach which only worked same-origin.
  useEffect(() => {
    if (!connectOpen || gitNamespaces.length <= namespaceCountOnOpenRef.current) return;
    const newest = gitNamespaces[gitNamespaces.length - 1];
    setConnectOpen(false);
    onChange?.(newest.id);
  }, [connectOpen, gitNamespaces, onChange]);

  // Auto-select the only connected namespace so the user doesn't have to click it.
  useEffect(() => {
    if (!isLoadingNamespaces && gitNamespaces.length === 1 && !value) {
      onChange?.(gitNamespaces[0].id);
    }
  }, [isLoadingNamespaces, gitNamespaces, value, onChange]);

  const selected = gitNamespaces.find((ns) => ns.id === value);

  const handleConnected = (namespaceId: string) => {
    setConnectOpen(false);
    refetch().then(() => onChange?.(namespaceId));
  };

  const isAutoConnecting =
    autoConnectAttempted.current && gitNamespaces.length === 0 && appInstallations.length === 1;

  if (isLoadingNamespaces || isAutoConnecting) {
    return (
      <div className='flex items-center gap-2 text-muted-foreground text-sm'>
        <Loader2 className='h-4 w-4 animate-spin' />
        {isAutoConnecting ? "Connecting GitHub account…" : "Loading accounts…"}
      </div>
    );
  }

  return (
    <div className='space-y-2'>
      <Label>GitHub account</Label>

      {/* Connected accounts list */}
      {gitNamespaces.length > 0 && (
        <div className='flex flex-col gap-1.5'>
          {gitNamespaces.map((ns) => {
            const isPAT = ns.slug === "pat";
            const isSelected = ns.id === value;
            return (
              <div
                key={ns.id}
                className={`flex items-center gap-3 rounded-lg border px-3 py-2.5 transition-colors ${
                  isSelected ? "border-primary bg-primary/5" : "border-border bg-card"
                }`}
              >
                <button
                  type='button'
                  onClick={() => onChange?.(ns.id)}
                  className='flex flex-1 items-center gap-3 text-left'
                >
                  <Github className='h-4 w-4 shrink-0 text-muted-foreground' />
                  <span className='flex-1 truncate font-medium text-sm'>{ns.name}</span>
                  <Badge variant='outline' className='shrink-0 gap-1 text-xs'>
                    {isPAT ? (
                      <>
                        <Key className='h-3 w-3' />
                        PAT
                      </>
                    ) : (
                      <>
                        <Github className='h-3 w-3' />
                        App
                      </>
                    )}
                  </Badge>
                  {isSelected && <div className='h-1.5 w-1.5 shrink-0 rounded-full bg-primary' />}
                </button>
                <button
                  type='button'
                  onClick={() => {
                    if (isSelected) onChange?.("");
                    setUserDisconnected(true);
                    deleteNamespace(ns.id);
                  }}
                  className='shrink-0 text-muted-foreground/50 transition-colors hover:text-destructive'
                  aria-label={`Remove ${ns.name}`}
                >
                  <X className='h-3.5 w-3.5' />
                </button>
              </div>
            );
          })}
        </div>
      )}

      {/* Add account button */}
      <Button
        variant='outline'
        size='sm'
        className='w-full gap-2'
        onClick={() => setConnectOpen(true)}
      >
        <Plus className='h-3.5 w-3.5' />
        {gitNamespaces.length === 0 ? "Connect a GitHub account" : "Connect another account"}
      </Button>

      {/* Clear selection if something is selected */}
      {selected && (
        <button
          type='button'
          onClick={() => onChange?.("")}
          className='flex items-center gap-1 text-muted-foreground text-xs hover:text-foreground'
        >
          <X className='h-3 w-3' />
          Clear selection
        </button>
      )}

      <ConnectDialog
        open={connectOpen}
        onClose={() => setConnectOpen(false)}
        onConnected={handleConnected}
      />
    </div>
  );
};
