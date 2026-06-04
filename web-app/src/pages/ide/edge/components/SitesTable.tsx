import { Plus } from "lucide-react";
import type React from "react";
import { useState } from "react";
import TableContentWrapper from "@/components/settings/components/TableContentWrapper";
import TableWrapper from "@/components/settings/components/TableWrapper";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import useSites from "@/hooks/api/cameras/useSites";
import type { Site } from "@/services/api/cameras";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import AddSiteDialog from "./AddSiteDialog";
import EditSiteDialog from "./EditSiteDialog";
import UnifiCredentialBanner from "./UnifiCredentialBanner";

const SitesTable: React.FC = () => {
  const { workspace } = useCurrentWorkspace();
  const { data: sites = [], isLoading, error, refetch } = useSites(workspace?.id);
  const [addOpen, setAddOpen] = useState(false);
  const [editingSite, setEditingSite] = useState<Site | null>(null);

  return (
    <div className='flex flex-col gap-3'>
      <UnifiCredentialBanner />
      <div className='flex justify-end'>
        <Button size='sm' onClick={() => setAddOpen(true)}>
          <Plus className='mr-1.5 h-4 w-4' />
          Add site
        </Button>
      </div>

      <TableWrapper>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Source</TableHead>
              <TableHead>Public IP</TableHead>
              <TableHead>Timezone</TableHead>
              <TableHead>Region</TableHead>
              <TableHead>Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableContentWrapper
              isEmpty={sites.length === 0}
              loading={isLoading}
              colSpan={6}
              noFoundTitle='No sites yet'
              noFoundDescription='Click "Add site" to provision one manually, or connect UniFi to import.'
              error={error?.message}
              onRetry={() => refetch()}
            >
              {sites.map((site) => (
                <TableRow
                  key={site.id}
                  className='cursor-pointer hover:bg-muted/40'
                  onClick={() => setEditingSite(site)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setEditingSite(site);
                    }
                  }}
                  role='button'
                  tabIndex={0}
                  data-testid={`sites-row-${site.id}`}
                >
                  <TableCell className='font-medium'>{site.name}</TableCell>
                  <TableCell>
                    <Badge variant={site.source === "unifi" ? "default" : "secondary"}>
                      {site.source === "unifi" ? "UniFi" : "Manual"}
                    </Badge>
                  </TableCell>
                  <TableCell className='font-mono text-muted-foreground text-xs'>
                    {site.public_ip ?? "—"}
                  </TableCell>
                  <TableCell className='text-muted-foreground'>{site.timezone}</TableCell>
                  <TableCell className='text-muted-foreground'>{site.region ?? "—"}</TableCell>
                  <TableCell className='text-muted-foreground'>
                    {new Date(site.created_at).toLocaleDateString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableContentWrapper>
          </TableBody>
        </Table>
      </TableWrapper>

      <AddSiteDialog open={addOpen} onOpenChange={setAddOpen} />
      <EditSiteDialog
        open={editingSite !== null}
        onOpenChange={(o) => {
          if (!o) setEditingSite(null);
        }}
        site={editingSite}
      />
    </div>
  );
};

export default SitesTable;
