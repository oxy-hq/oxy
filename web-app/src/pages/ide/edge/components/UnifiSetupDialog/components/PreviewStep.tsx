import { AlertCircle } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/shadcn/alert";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import useUnifiImport from "@/hooks/api/cameras/useUnifiImport";
import type { UnifiImportResult, UnifiPreviewResult } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

type Props = {
  /**
   * UniFi API key. Undefined when the dialog is in `scan-stored` mode —
   * the backend then uses the stored workspace credential for both the
   * preview that produced `data` and the import that follows.
   */
  apiKey?: string;
  data: UnifiPreviewResult;
  onBack: () => void;
  onImported: (result: UnifiImportResult) => void;
};

const PreviewStep: React.FC<Props> = ({ apiKey, data, onBack, onImported }) => {
  const { workspace } = useCurrentWorkspace();
  const importMut = useUnifiImport(workspace?.id);
  const [filter, setFilter] = useState("");

  const filtered = filter.trim()
    ? data.sites.filter((s) => s.name.toLowerCase().includes(filter.trim().toLowerCase()))
    : data.sites;

  const handleImport = async () => {
    const result = await importMut.mutateAsync({
      apiKey,
      siteFilter: filter.trim() || undefined
    });
    onImported(result);
  };

  return (
    <div className='flex flex-col gap-4'>
      <div className='text-muted-foreground text-sm'>
        Found <span className='font-medium text-foreground'>{data.total_sites}</span> sites with{" "}
        <span className='font-medium text-foreground'>{data.total_cameras}</span> cameras across
        your UniFi account.
      </div>

      <div className='flex flex-col gap-2'>
        <Label htmlFor='unifi-site-filter'>Site filter (optional)</Label>
        <Input
          id='unifi-site-filter'
          placeholder='Substring match on site name, e.g. "pokehouse"'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <p className='text-muted-foreground text-xs'>
          Leave blank to import everything. The filter is matched server-side at import time.
        </p>
      </div>

      <div className='max-h-60 overflow-auto rounded-md border'>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Site</TableHead>
              <TableHead>Hardware</TableHead>
              <TableHead>IP</TableHead>
              <TableHead className='text-right'>Cameras</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filtered.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className='text-center text-muted-foreground'>
                  No sites match the filter.
                </TableCell>
              </TableRow>
            ) : (
              filtered.map((s) => (
                <TableRow key={s.unifi_console_id}>
                  <TableCell className='font-medium'>{s.name}</TableCell>
                  <TableCell className='text-muted-foreground'>{s.hardware_model ?? "—"}</TableCell>
                  <TableCell className='text-muted-foreground'>{s.public_ip ?? "—"}</TableCell>
                  <TableCell className='text-right'>
                    {s.online_camera_count}/{s.camera_count}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      {importMut.isError && (
        <Alert variant='destructive'>
          <AlertCircle />
          <AlertTitle>Import failed</AlertTitle>
          <AlertDescription>{(importMut.error as Error).message}</AlertDescription>
        </Alert>
      )}

      <div className='flex justify-between'>
        <Button type='button' variant='ghost' onClick={onBack} disabled={importMut.isPending}>
          Back
        </Button>
        <Button onClick={handleImport} disabled={importMut.isPending || filtered.length === 0}>
          {importMut.isPending ? (
            <Spinner />
          ) : (
            `Import ${filtered.length} site${filtered.length === 1 ? "" : "s"}`
          )}
        </Button>
      </div>
    </div>
  );
};

export default PreviewStep;
