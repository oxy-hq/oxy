import { Monitor, Smartphone, Trash2 } from "lucide-react";
import { type KeyboardEvent, useRef, useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Switch } from "@/components/ui/shadcn/switch";
import { dateAgo, parseUtcTimestamp } from "@/libs/utils/date";
import type { DevicePatch, NotificationDevice } from "@/types/notifications";
import { FLOORS } from "../../../utils";
import { fallbackName, floorLevel, floorValue, osStatus } from "../utils";

interface DeviceRowProps {
  device: NotificationDevice;
  /** The row this browser registered. */
  thisDevice: boolean;
  onChange: (patch: DevicePatch) => void;
  onRemove: () => void;
}

/**
 * One device push reaches: its name (click to rename), whether it is this
 * browser, what its own settings block, and when it was last seen; then push
 * on or off here, the lowest level sent here, and remove.
 */
export function DeviceRow({ device, thisDevice, onChange, onRemove }: DeviceRowProps) {
  const fallback = fallbackName(device.platform);
  const name = device.name ?? fallback;
  const status = osStatus(device);
  const seen = parseUtcTimestamp(device.last_seen_at);
  const testId = `notifications-device-${device.id}`;
  const Icon = device.platform === "web" ? Monitor : Smartphone;

  return (
    <div className='flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3' data-testid={testId}>
      <div className='flex min-w-48 flex-1 items-center gap-3'>
        <Icon className='size-4 shrink-0 text-muted-foreground' aria-hidden />
        <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
          <DeviceName
            key={name}
            name={name}
            testId={testId}
            onRename={(next) => onChange({ name: next })}
          />
          <span className='flex flex-wrap items-center gap-x-2 gap-y-1 text-muted-foreground text-xs'>
            {thisDevice && (
              <Badge variant='secondary' data-testid={`${testId}-this`}>
                This device
              </Badge>
            )}
            {status && (
              <span className='text-warning' data-testid={`${testId}-status`}>
                {status}
              </span>
            )}
            {seen && <span data-testid={`${testId}-seen`}>Seen {dateAgo(seen)}</span>}
          </span>
        </div>
      </div>
      <div className='flex items-center gap-2 pl-7 sm:pl-0'>
        <Select
          value={floorValue(device.min_level)}
          onValueChange={(value) => onChange({ min_level: floorLevel(value) })}
        >
          <SelectTrigger
            size='sm'
            className='w-56'
            aria-label={`${name}: send`}
            data-testid={`${testId}-floor`}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {FLOORS.map((floor) => (
              <SelectItem
                key={floor.value}
                value={floorValue(floor.value)}
                data-testid={`${testId}-floor-${floor.value}`}
              >
                {floor.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Switch
          checked={device.enabled}
          onCheckedChange={(enabled) => onChange({ enabled })}
          aria-label={`${name}: push`}
          data-testid={`${testId}-enabled`}
        />
        <Button
          variant='ghost'
          size='icon'
          className='size-8'
          onClick={onRemove}
          aria-label={`Remove ${name}`}
          data-testid={`${testId}-remove`}
        >
          <Trash2 />
        </Button>
      </div>
    </div>
  );
}

/** The name as a quiet field: saved when left or on Enter; Escape or empty puts it back. */
function DeviceName({
  name,
  testId,
  onRename
}: {
  name: string;
  testId: string;
  onRename: (name: string) => void;
}) {
  const [draft, setDraft] = useState(name);
  // Escape blurs too, and the blur must not save the draft it just threw away.
  const cancelled = useRef(false);
  const commit = () => {
    const next = draft.trim();
    if (cancelled.current || !next) {
      cancelled.current = false;
      setDraft(name);
    } else if (next !== name) {
      onRename(next);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") cancelled.current = true;
    if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
  };

  return (
    <Input
      value={draft}
      maxLength={120}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={onKeyDown}
      aria-label='Device name'
      className='-ml-2 h-7 border-transparent px-2 text-sm shadow-none hover:border-input focus-visible:border-input'
      data-testid={`${testId}-name`}
    />
  );
}
