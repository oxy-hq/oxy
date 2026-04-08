import { Building2, Github, Key, Loader2, Plus, User, X } from "lucide-react";
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
  useCreatePATNamespace,
  useDeleteGitNamespace,
  useGitHubNamespaces,
  usePickNamespaceInstallation
} from "@/hooks/api/github";
import { GITHUB_OAUTH_CALLBACK_MESSAGE } from "@/pages/github/callback";
import { GitHubApiService } from "@/services/api";
import type { OAuthInstallation } from "@/types/github";

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
  const [oauthPopupOpened, setOauthPopupOpened] = useState(false);
  const [oauthInstallations, setOauthInstallations] = useState<OAuthInstallation[] | null>(null);
  const [selectionToken, setSelectionToken] = useState<string>("");
  const [oauthError, setOauthError] = useState<string | null>(null);
  // Shown when the OAuth flow finds no installations for the user
  const [notInstalled, setNotInstalled] = useState(false);

  const oauthPopupRef = useRef<Window | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const { mutate: createPAT, isPending: isPATLoading } = useCreatePATNamespace();
  const { mutate: connectFromOAuth, isPending: isOAuthConnecting } = useConnectNamespaceFromOAuth();
  const { mutate: pickInstallation, isPending: isPickingInstallation } =
    usePickNamespaceInstallation();

  useEffect(() => {
    if (!oauthPopupOpened) return;

    const allowedOrigin = import.meta.env.VITE_GITHUB_CALLBACK_ORIGIN || window.location.origin;
    const handleMessage = (e: MessageEvent) => {
      if (e.origin !== window.location.origin && e.origin !== allowedOrigin) return;
      if (e.data?.type !== GITHUB_OAUTH_CALLBACK_MESSAGE) return;
      const { code, state } = e.data as { type: string; code: string; state: string };
      if (!code || !state) return;

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
              setSelectionToken(result.selection_token);
            } else {
              setNotInstalled(true);
            }
          },
          onError: (err) => setOauthError(err.message || "Authentication failed.")
        }
      );
    };

    window.addEventListener("message", handleMessage);
    const closedCheck = setInterval(() => {
      if (oauthPopupRef.current?.closed) {
        setOauthPopupOpened(false);
        oauthPopupRef.current = null;
      }
    }, 500);
    return () => {
      window.removeEventListener("message", handleMessage);
      clearInterval(closedCheck);
    };
  }, [oauthPopupOpened, connectFromOAuth, onConnected]);

  const handlePATSubmit = () => {
    if (!patToken.trim()) return;
    setPATError(null);
    createPAT(patToken.trim(), {
      onSuccess: (ns) => {
        setPATToken("");
        onConnected(ns.id);
      },
      onError: (e) => setPATError(e.message || "Invalid token — needs 'repo' scope.")
    });
  };

  const handleSignIn = async () => {
    setOauthError(null);
    setOauthInstallations(null);
    setSelectionToken("");
    setNotInstalled(false);
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

  const handlePickInstallation = (installationId: number) => {
    pickInstallation(
      { installation_id: installationId, selection_token: selectionToken },
      {
        onSuccess: (ns) => {
          setOauthInstallations(null);
          setSelectionToken("");
          onConnected(ns.id);
        },
        onError: (err) => setOauthError(err.message || "Failed to connect.")
      }
    );
  };

  const handleClose = () => {
    setPATToken("");
    setPATError(null);
    setOauthPopupOpened(false);
    setOauthInstallations(null);
    setSelectionToken("");
    setOauthError(null);
    setNotInstalled(false);
    onClose();
  };

  const anyBusy = oauthPopupOpened || isOAuthConnecting || isPickingInstallation;

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
            {/* Step 1: Sign in */}
            {!notInstalled && !oauthInstallations && (
              <Button className='w-full gap-2' onClick={handleSignIn} disabled={anyBusy}>
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

            {/* App not installed on any of the user's accounts */}
            {notInstalled && (
              <div className='space-y-3'>
                <div className='rounded-lg border border-border/60 bg-muted/30 px-3 py-3'>
                  <p className='font-medium text-sm'>GitHub App not installed</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    The GitHub App is not installed on any account you belong to. Ask your admin to
                    install it on the organization.
                  </p>
                </div>
                <button
                  type='button'
                  onClick={() => setNotInstalled(false)}
                  className='w-full text-center text-muted-foreground text-xs hover:text-foreground'
                >
                  ← Back
                </button>
              </div>
            )}

            {/* Multiple installations — pick one */}
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
  const [connectOpen, setConnectOpen] = useState(false);

  // Auto-select the only connected namespace so the user doesn't have to click it.
  useEffect(() => {
    if (!isLoadingNamespaces && gitNamespaces.length === 1 && !value) {
      onChange?.(gitNamespaces[0].id);
    }
  }, [isLoadingNamespaces, gitNamespaces, value, onChange]);

  const handleConnected = (namespaceId: string) => {
    setConnectOpen(false);
    refetch().then(() => onChange?.(namespaceId));
  };

  if (isLoadingNamespaces) {
    return (
      <div className='flex items-center gap-2 text-muted-foreground text-sm'>
        <Loader2 className='h-4 w-4 animate-spin' />
        Loading accounts…
      </div>
    );
  }

  return (
    <div className='space-y-2'>
      <Label>GitHub account</Label>

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

      <Button
        variant='outline'
        size='sm'
        className='w-full gap-2'
        onClick={() => setConnectOpen(true)}
      >
        <Plus className='h-3.5 w-3.5' />
        {gitNamespaces.length === 0 ? "Connect a GitHub account" : "Connect another account"}
      </Button>

      {value && (
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
