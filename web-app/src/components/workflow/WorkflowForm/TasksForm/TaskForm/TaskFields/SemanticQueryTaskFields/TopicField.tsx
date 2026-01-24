import React from "react";
import { Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Loader2 } from "lucide-react";
import { TopicFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const TopicField: React.FC<TopicFieldProps> = ({
  taskPath,
  control,
  register,
  topicItems,
  topicsLoading,
  topicsError,
  taskErrors,
}) => {
  const renderTopicInput = () => {
    if (topicsLoading) {
      return (
        <div className="flex items-center gap-2 h-10 px-3 border rounded-md bg-muted">
          <Loader2 className="w-4 h-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading topics...
          </span>
        </div>
      );
    }

    if (topicsError) {
      return (
        <Input
          id={`${taskPath}.topic`}
          placeholder="Enter topic path"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.topic`, {
            required: "Topic is required",
          })}
        />
      );
    }

    return (
      <Controller
        control={control}
        // @ts-expect-error - dynamic field path
        name={`${taskPath}.topic`}
        rules={{ required: "Topic is required" }}
        render={({ field }) => {
          const value = (field.value as string) ?? "";
          const items = getItemsWithUnknownValue(topicItems, value);
          return (
            <Combobox
              items={items}
              value={value}
              onValueChange={field.onChange}
              placeholder="Select topic..."
              searchPlaceholder="Search topics..."
            />
          );
        }}
      />
    );
  };

  return (
    <div className="space-y-2">
      <Label htmlFor={`${taskPath}.topic`}>Topic</Label>
      {renderTopicInput()}
      {taskErrors?.topic && (
        <p className="text-sm text-red-500">
          {taskErrors.topic.message as string}
        </p>
      )}
    </div>
  );
};
