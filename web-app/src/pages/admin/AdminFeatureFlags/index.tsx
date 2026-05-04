import { Flag } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Badge } from "@/components/ui/shadcn/badge";
import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Switch } from "@/components/ui/shadcn/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { useFeatureFlags, useUpdateFeatureFlag } from "@/hooks/api/featureFlags";

type PendingToggle = { key: string; nextValue: boolean };

function formatUpdatedAt(value: string | null): string {
  if (!value) return "—";
  return new Date(value).toLocaleString();
}

export default function AdminFeatureFlags() {
  const { data: flags = [], isLoading } = useFeatureFlags();
  const updateFlag = useUpdateFeatureFlag();
  const [pending, setPending] = useState<PendingToggle | null>(null);

  const confirmToggle = () => {
    if (!pending) return;
    const { key, nextValue } = pending;
    updateFlag.mutate(
      { key, enabled: nextValue },
      {
        onSuccess: (updated) => {
          toast.success(`${updated.key} is now ${updated.enabled ? "on" : "off"}.`);
        },
        onError: (err) => {
          const message = err instanceof Error ? err.message : "Failed to update flag.";
          toast.error(message);
        },
        onSettled: () => {
          setPending(null);
        }
      }
    );
  };

  return (
    <div className='mx-auto max-w-5xl p-6'>
      <div className='mb-6'>
        <h1 className='font-semibold text-2xl tracking-tight'>Feature flags</h1>
        <p className='mt-1 text-muted-foreground text-sm'>
          Toggle backend feature flags. Changes apply immediately on this server.
        </p>
      </div>

      <Card>
        <CardContent className='p-0'>
          {isLoading ? (
            <div className='flex items-center justify-center gap-2 py-16 text-muted-foreground text-sm'>
              <Spinner /> Loading…
            </div>
          ) : flags.length === 0 ? (
            <div className='flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground'>
              <Flag className='size-8' />
              <p className='text-sm'>No feature flags defined.</p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Flag</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead>Default</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className='text-right'>Enabled</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {flags.map((flag) => (
                  <TableRow key={flag.key} className='hover:bg-muted/40'>
                    <TableCell>
                      <span className='font-mono text-sm'>{flag.key}</span>
                    </TableCell>
                    <TableCell className='whitespace-normal break-words text-muted-foreground text-sm'>
                      {flag.description}
                    </TableCell>
                    <TableCell>
                      <Badge variant={flag.default ? "default" : "outline"}>
                        {flag.default ? "On" : "Off"}
                      </Badge>
                    </TableCell>
                    <TableCell className='text-muted-foreground text-sm tabular-nums'>
                      {formatUpdatedAt(flag.updated_at)}
                    </TableCell>
                    <TableCell className='text-right'>
                      <Switch
                        checked={flag.enabled}
                        onCheckedChange={(next) => setPending({ key: flag.key, nextValue: next })}
                        disabled={updateFlag.isPending}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <AlertDialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open && !updateFlag.isPending) setPending(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Turn {pending?.nextValue ? "on" : "off"} {pending?.key}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              This change applies immediately on this server and affects all organizations.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={updateFlag.isPending}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={updateFlag.isPending}
              onClick={(event) => {
                event.preventDefault();
                confirmToggle();
              }}
            >
              {updateFlag.isPending ? "Saving…" : "Confirm"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
