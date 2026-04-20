import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Label } from "@/components/ui/shadcn/label";
import { useDeleteGitNamespace, useGitHubNamespaces } from "@/hooks/api/github";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { GitHubNamespace } from "@/types/github";
import AddNamespaceMenu from "./AddNamespaceMenu";
import AddViaInstallationDialog from "./AddViaInstallationDialog";
import AddViaPATDialog from "./AddViaPATDialog";
import NamespaceList from "./NamespaceList";

type DialogState =
  | { kind: "closed" }
  | { kind: "menu" }
  | { kind: "pat" }
  | { kind: "installation" };

interface Props {
  value?: string;
  onChange?: (value: string) => void;
}

export const GitNamespaceSelection = ({ value, onChange }: Props) => {
  const { org } = useCurrentOrg();
  const orgId = org?.id ?? "";

  const {
    data: gitNamespaces = [],
    isPending: isLoadingNamespaces,
    refetch
  } = useGitHubNamespaces(orgId);
  const { mutate: deleteNamespace } = useDeleteGitNamespace();

  const [dialog, setDialog] = useState<DialogState>({ kind: "closed" });

  // Snapshot the namespace count when the connect dialog opens so we can
  // detect when a new namespace is added (e.g. via popup on a different domain).
  const namespaceCountOnOpenRef = useRef(0);
  useEffect(() => {
    if (dialog.kind !== "closed") namespaceCountOnOpenRef.current = gitNamespaces.length;
  }, [dialog.kind, gitNamespaces.length]);

  // When a new namespace appears while the connect dialog is open, treat it as
  // a successful connection and close the dialog automatically.
  useEffect(() => {
    if (dialog.kind === "closed" || gitNamespaces.length <= namespaceCountOnOpenRef.current) return;
    const newest = gitNamespaces[gitNamespaces.length - 1];
    setDialog({ kind: "closed" });
    onChange?.(newest.id);
  }, [dialog.kind, gitNamespaces, onChange]);

  // Auto-select the only connected namespace so the user doesn't have to click it.
  useEffect(() => {
    if (!isLoadingNamespaces && gitNamespaces.length === 1 && !value) {
      onChange?.(gitNamespaces[0].id);
    }
  }, [isLoadingNamespaces, gitNamespaces, value, onChange]);

  const handleConnected = (namespaceId: string) => {
    setDialog({ kind: "closed" });
    refetch().then(() => onChange?.(namespaceId));
  };

  const handleDelete = (ns: GitHubNamespace) => {
    if (ns.id === value) onChange?.("");
    deleteNamespace({ orgId, id: ns.id });
  };

  return (
    <div className='space-y-2'>
      <Label>GitHub account</Label>

      <NamespaceList
        namespaces={gitNamespaces}
        isLoading={isLoadingNamespaces}
        value={value}
        onChange={onChange}
        onDelete={handleDelete}
        onAdd={() => setDialog({ kind: "menu" })}
      />

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

      <AddNamespaceMenu
        open={dialog.kind === "menu"}
        onClose={() => setDialog({ kind: "closed" })}
        onSelectApp={() => setDialog({ kind: "installation" })}
        onSelectPAT={() => setDialog({ kind: "pat" })}
      />

      <AddViaPATDialog
        orgId={orgId}
        open={dialog.kind === "pat"}
        onClose={() => setDialog({ kind: "closed" })}
        onConnected={handleConnected}
      />

      <AddViaInstallationDialog
        orgId={orgId}
        open={dialog.kind === "installation"}
        onClose={() => setDialog({ kind: "closed" })}
        onConnected={handleConnected}
      />
    </div>
  );
};
