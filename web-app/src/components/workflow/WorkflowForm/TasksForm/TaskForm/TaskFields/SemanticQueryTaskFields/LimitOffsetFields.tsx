import React from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { LimitOffsetFieldsProps } from "./types";

export const LimitOffsetFields: React.FC<LimitOffsetFieldsProps> = ({
  taskPath,
  register,
}) => {
  return (
    <div className="grid grid-cols-2 gap-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.limit`}>Limit</Label>
        <Input
          id={`${taskPath}.limit`}
          type="number"
          min="0"
          placeholder="Optional limit"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.limit`, {
            valueAsNumber: true,
          })}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.offset`}>Offset</Label>
        <Input
          id={`${taskPath}.offset`}
          type="number"
          min="0"
          placeholder="Optional offset"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.offset`, {
            valueAsNumber: true,
          })}
        />
      </div>
    </div>
  );
};
