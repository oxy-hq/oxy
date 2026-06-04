import { Camera, Cpu, MapPin } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import CamerasTable from "./components/CamerasTable";
import EdgeBoxesTable from "./components/EdgeBoxesTable";
import SetupNeededBanner from "./components/SetupNeededBanner";
import SitesTable from "./components/SitesTable";

/**
 * Devices — Sites / Edge boxes / Cameras as tables. Operator's
 * primary surface for adding and configuring fleet hardware.
 *
 * Each inner table honors the global site picker independently —
 * SitesTable scopes its list to the picked site (or shows all),
 * EdgeBoxesTable filters by site, CamerasTable filters by site.
 * The per-table site filtering is handled inside each component
 * (they already had per-row site columns) so this page stays a
 * thin tab shell.
 */

type Inner = "sites" | "edge-boxes" | "cameras";

const DevicesPage: React.FC = () => {
  const [searchParams] = useSearchParams();
  // Initial tab — Cameras when we landed via the
  // SetupNeededBanner's review link, Edge boxes otherwise.
  // After the first render, the operator can switch freely.
  const [active, setActive] = useState<Inner>(() =>
    searchParams.get("review") === "1" ? "cameras" : "edge-boxes"
  );

  return (
    <div className='flex h-full min-h-0 flex-col gap-3 p-4' data-testid='edge-devices'>
      <SetupNeededBanner />
      <Tabs value={active} onValueChange={(v) => setActive(v as Inner)} className='flex flex-col'>
        <TabsList>
          <TabsTrigger value='sites'>
            <MapPin className='h-3.5 w-3.5' /> Sites
          </TabsTrigger>
          <TabsTrigger value='edge-boxes'>
            <Cpu className='h-3.5 w-3.5' /> Edge boxes
          </TabsTrigger>
          <TabsTrigger value='cameras'>
            <Camera className='h-3.5 w-3.5' /> Cameras
          </TabsTrigger>
        </TabsList>
        <TabsContent value='sites' className='mt-4'>
          <SitesTable />
        </TabsContent>
        <TabsContent value='edge-boxes' className='mt-4'>
          <EdgeBoxesTable />
        </TabsContent>
        <TabsContent value='cameras' className='mt-4'>
          <CamerasTable />
        </TabsContent>
      </Tabs>
    </div>
  );
};

export default DevicesPage;
