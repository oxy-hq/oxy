import { useRef, useState } from "react";
import { useRenameWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import type { WorkspaceSummary } from "@/services/api/workspaces";

type RenameError = { response?: { data?: unknown; status?: number } };

export function useRenameForm(workspace: WorkspaceSummary) {
  const [isOpen, setIsOpen] = useState(false);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const { mutate, isPending } = useRenameWorkspace();

  const open = () => {
    setValue(workspace.name);
    setError(null);
    setIsOpen(true);
    setTimeout(() => inputRef.current?.select(), 0);
  };

  const close = () => setIsOpen(false);

  const submit = () => {
    const name = value.trim();
    if (!name || name === workspace.name) {
      setIsOpen(false);
      return;
    }
    if (!workspace.org_id) {
      setError("Workspace has no organization");
      return;
    }
    mutate(
      { orgId: workspace.org_id, id: workspace.id, name },
      {
        onSuccess: () => setIsOpen(false),
        onError: (err) => setError(resolveRenameError(err))
      }
    );
  };

  return { isOpen, value, setValue, error, isPending, inputRef, open, close, submit };
}

function resolveRenameError(err: unknown): string {
  const response = (err as RenameError)?.response;
  const body = response?.data;
  if (typeof body === "string" && body.length > 0) return body;
  if (response?.status === 409) return "A workspace with that name already exists.";
  return "Failed to rename workspace";
}
