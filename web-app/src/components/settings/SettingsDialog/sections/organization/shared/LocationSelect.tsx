import { useMemo } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { LocationRow } from "@/types/operatingGraph";
import { LOCATION_STATUS_LABELS, locationTree } from "../Locations/utils";

const isClosed = (status: LocationRow["status"]) =>
  status === "archived" || status === "terminated";

/** Radix Select can't carry an empty value, so "no location" needs a name. Ids are uuids. */
export const NO_LOCATION = "__none__";

/**
 * The org's places in tree order, children indented under their parent so a
 * manager picking "Clovis" sees which region it sits in.
 */
export function LocationSelect({
  locations,
  value,
  onValueChange,
  allowNone = false,
  noneLabel = "None",
  exclude,
  includeClosed = false,
  placeholder = "Pick a location",
  id,
  disabled,
  testId
}: {
  locations: LocationRow[];
  value: string;
  onValueChange: (value: string) => void;
  /** Offer `NO_LOCATION` first — "no parent", or a kiosk with no place. */
  allowNone?: boolean;
  noneLabel?: string;
  /** Ids not to offer — a location and its descendants, when picking its parent. */
  exclude?: Set<string>;
  /**
   * Offer archived and terminated places too. Off by default: nobody is
   * rostered at a closed store and no kiosk sits in one. The parent picker
   * turns it on — a region that closed still has children to name.
   */
  includeClosed?: boolean;
  placeholder?: string;
  id?: string;
  disabled?: boolean;
  testId: string;
}) {
  const rows = useMemo(() => {
    // Closed places drop out BEFORE the tree is built, so an open store under
    // an archived region is offered as a root rather than indented under a
    // parent row that is not there. The current value stays visible even when
    // closed, so an existing choice is never silently blanked.
    const offered = locations.filter((l) => includeClosed || !isClosed(l.status) || l.id === value);
    return locationTree(offered).filter((row) => !exclude?.has(row.location.id));
  }, [locations, exclude, includeClosed, value]);
  const nothingToPick = rows.length === 0 && !allowNone;
  return (
    <Select value={value} onValueChange={onValueChange} disabled={disabled || nothingToPick}>
      <SelectTrigger id={id} className='w-full' data-testid={testId}>
        <SelectValue placeholder={nothingToPick ? "No locations yet" : placeholder} />
      </SelectTrigger>
      <SelectContent>
        {allowNone && <SelectItem value={NO_LOCATION}>{noneLabel}</SelectItem>}
        {rows.map(({ location, depth }) => (
          <SelectItem key={location.id} value={location.id}>
            <span style={{ paddingLeft: `${depth * 0.75}rem` }}>{location.name}</span>
            {location.status !== "open" && (
              <span className='ml-2 text-muted-foreground text-xs'>
                {LOCATION_STATUS_LABELS[location.status]}
              </span>
            )}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
