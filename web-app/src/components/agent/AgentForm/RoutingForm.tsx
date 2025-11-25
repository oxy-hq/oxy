import React from "react";
import { useFormContext, useFieldArray } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { Button } from "@/components/ui/shadcn/button";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Plus, Trash2 } from "lucide-react";
import { CardTitle } from "@/components/ui/shadcn/card";
import { AgentFormData } from "./index";

export const RoutingForm: React.FC = () => {
  const { control, register } = useFormContext<AgentFormData>();

  const {
    fields: routeFields,
    append: appendRoute,
    remove: removeRoute,
  } = useFieldArray({
    control,
    name: "routes" as never,
  });

  return (
    <div className="space-y-6">
      <CardTitle>Routing Configuration</CardTitle>

      <div className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="system_instructions">System Instructions</Label>
          <Textarea
            id="system_instructions"
            placeholder="Instructions for the routing agent..."
            rows={4}
            {...register("system_instructions")}
          />
          <p className="text-sm text-muted-foreground">
            Default: "You are a routing agent. Your job is to route the task to
            the correct tool..."
          </p>
        </div>

        <div className="flex items-center justify-between">
          <div>
            <h4 className="font-medium">Routes *</h4>
            <p className="text-sm text-muted-foreground">
              List of agent routes to choose from
            </p>
          </div>
          <Button
            type="button"
            onClick={() => appendRoute("")}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-2" />
            Add Route
          </Button>
        </div>

        {routeFields.length === 0 && (
          <p className="text-center text-muted-foreground py-4">
            No routes defined. Add at least one route.
          </p>
        )}

        {routeFields.map((field, index) => (
          <div key={field.id} className="flex items-center gap-2">
            <div className="flex-1">
              <FilePathAutocompleteInput
                fileExtension=".agent.yml"
                datalistId={`route-${index}`}
                placeholder="Path to agent (e.g., agents/sql-agent.agent.yml)"
                {...register(`routes.${index}`)}
              />
            </div>
            <Button
              type="button"
              onClick={() => removeRoute(index)}
              variant="outline"
              size="sm"
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
        ))}
      </div>

      <div className="space-y-2">
        <Label htmlFor="route_fallback">Route Fallback</Label>
        <FilePathAutocompleteInput
          id="route_fallback"
          fileExtension=".agent.yml"
          datalistId="route-fallback"
          placeholder="Optional fallback route"
          {...register("route_fallback")}
        />
        <p className="text-sm text-muted-foreground">
          Agent to use if no route matches
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="embed_model">Embedding Model</Label>
          <Input
            id="embed_model"
            placeholder="text-embedding-3-small"
            defaultValue="text-embedding-3-small"
            {...register("embed_model")}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="top_k">Top K</Label>
          <Input
            id="top_k"
            type="number"
            min="1"
            defaultValue={4}
            {...register("top_k", {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor="factor">Factor</Label>
          <Input
            id="factor"
            type="number"
            min="1"
            defaultValue={5}
            {...register("factor", {
              valueAsNumber: true,
            })}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="n_dims">Dimensions</Label>
          <Input
            id="n_dims"
            type="number"
            min="1"
            defaultValue={512}
            {...register("n_dims", {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>

      <div className="flex items-center space-x-2">
        <input
          type="checkbox"
          id="synthesize_results"
          defaultChecked={true}
          {...register("synthesize_results")}
          className="rounded"
        />
        <Label htmlFor="synthesize_results">Synthesize Results</Label>
      </div>
    </div>
  );
};
