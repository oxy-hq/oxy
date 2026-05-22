import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Switch } from "@/components/ui/shadcn/switch";
import { useAgenticWorkflowFiles } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";
import {
  useAirwayFiles,
  useCreateSchedule,
  useUpdateSchedule
} from "@/hooks/api/schedules/useSchedules";
import type { Schedule, ScheduleInput, ScheduleTargetKind } from "@/types/schedule";
import CronBuilder from "./CronBuilder";

const TIMEZONES: string[] = (() => {
  if ("supportedValuesOf" in Intl) {
    return (Intl as unknown as { supportedValuesOf: (k: string) => string[] }).supportedValuesOf(
      "timeZone"
    );
  }
  return ["UTC"];
})();

const FREE_TEXT = "__free_text__";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Present = edit; absent = create. */
  schedule?: Schedule | null;
}

const ScheduleDialog: React.FC<Props> = ({ open, onOpenChange, schedule }) => {
  const isEdit = !!schedule;
  const createMut = useCreateSchedule();
  const updateMut = useUpdateSchedule();

  const [name, setName] = useState("");
  const [targetKind, setTargetKind] = useState<ScheduleTargetKind>("workflow");
  const [targetRef, setTargetRef] = useState("");
  const [cron, setCron] = useState("0 9 * * *");
  const [timezone, setTimezone] = useState("UTC");
  const [enabled, setEnabled] = useState(true);

  // Reset/prefill whenever the dialog opens.
  useEffect(() => {
    if (!open) return;
    setName(schedule?.name ?? "");
    setTargetKind(schedule?.target_kind ?? "workflow");
    setTargetRef(schedule?.target_ref ?? "");
    setCron(schedule?.cron_expr ?? "0 9 * * *");
    setTimezone(schedule?.timezone ?? "UTC");
    setEnabled(schedule?.enabled ?? true);
  }, [open, schedule]);

  const { data: workflowFiles } = useAgenticWorkflowFiles();
  const { data: airwayFiles } = useAirwayFiles();
  const refs = useMemo(
    () =>
      (targetKind === "workflow"
        ? workflowFiles?.map((f) => f.path)
        : airwayFiles?.map((f) => f.path)) ?? [],
    [targetKind, workflowFiles, airwayFiles]
  );
  // The picker selects a known ref or "free text"; in free-text mode the
  // ref comes from the input below.
  const isKnownRef = refs.includes(targetRef);
  const [freeText, setFreeText] = useState(false);
  useEffect(() => {
    if (open) setFreeText(!!targetRef && !isKnownRef);
    // Only re-evaluate when the dialog (re)opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, targetRef, isKnownRef]);

  const submit = async () => {
    if (!name.trim()) {
      toast.error("Name is required");
      return;
    }
    if (!targetRef.trim()) {
      toast.error("Target is required");
      return;
    }
    const input: ScheduleInput = {
      name: name.trim(),
      target_kind: targetKind,
      target_ref: targetRef.trim(),
      cron_expr: cron.trim(),
      timezone,
      enabled
    };
    try {
      if (isEdit && schedule) {
        await updateMut.mutateAsync({ id: schedule.id, input });
      } else {
        await createMut.mutateAsync(input);
      }
      onOpenChange(false);
    } catch {
      // Surfaced by the mutation's onError toast (incl. backend 400s).
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-h-[85vh] overflow-y-auto sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>{isEdit ? "Edit schedule" : "Create schedule"}</DialogTitle>
          <DialogDescription>
            Run a workflow or airway pipeline on a recurring cron schedule.
          </DialogDescription>
        </DialogHeader>

        <div className='flex flex-col gap-4'>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='sched-name'>Name</Label>
            <Input
              id='sched-name'
              placeholder='e.g., Daily revenue report'
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div className='flex flex-wrap items-end gap-3'>
            <div className='flex flex-col gap-2'>
              <Label>Target type</Label>
              <Select
                value={targetKind}
                onValueChange={(v) => {
                  setTargetKind(v as ScheduleTargetKind);
                  setTargetRef("");
                  setFreeText(false);
                }}
              >
                <SelectTrigger className='w-40'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value='workflow'>Workflow</SelectItem>
                  <SelectItem value='airway'>Airway</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className='flex min-w-60 flex-1 flex-col gap-2'>
              <Label>Target file</Label>
              {freeText ? (
                <Input
                  placeholder='workspace-relative path'
                  className='font-mono'
                  value={targetRef}
                  onChange={(e) => setTargetRef(e.target.value)}
                />
              ) : (
                <Select
                  value={isKnownRef ? targetRef : ""}
                  onValueChange={(v) => {
                    if (v === FREE_TEXT) {
                      setFreeText(true);
                      setTargetRef("");
                    } else {
                      setTargetRef(v);
                    }
                  }}
                >
                  <SelectTrigger>
                    <SelectValue placeholder='Select a file' />
                  </SelectTrigger>
                  <SelectContent>
                    {refs.map((r) => (
                      <SelectItem key={r} value={r}>
                        {r}
                      </SelectItem>
                    ))}
                    <SelectItem value={FREE_TEXT}>Other (type a path)…</SelectItem>
                  </SelectContent>
                </Select>
              )}
            </div>
          </div>

          <CronBuilder value={cron} onChange={setCron} />

          <div className='flex flex-col gap-2'>
            <Label>Timezone</Label>
            <Select value={timezone} onValueChange={setTimezone}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent className='max-h-72'>
                {TIMEZONES.map((tz) => (
                  <SelectItem key={tz} value={tz}>
                    {tz}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className='flex items-center justify-between'>
            <Label htmlFor='sched-enabled'>Enabled</Label>
            <Switch id='sched-enabled' checked={enabled} onCheckedChange={setEnabled} />
          </div>
        </div>

        <DialogFooter>
          <Button variant='outline' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={createMut.isPending || updateMut.isPending}>
            {isEdit ? "Save changes" : "Create schedule"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default ScheduleDialog;
