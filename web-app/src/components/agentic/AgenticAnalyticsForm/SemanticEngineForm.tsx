import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Controller, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { SEMANTIC_ENGINE_VENDORS } from "./constants";
import type { AgenticFormData } from "./index";

export const SemanticEngineForm: React.FC = () => {
  const { register, control, watch, setValue } = useFormContext<AgenticFormData>();
  const [isOpen, setIsOpen] = useState(false);
  const [showSection, setShowSection] = useState(!!watch("semantic_engine"));

  const vendor = watch("semantic_engine.vendor");
  const isLooker = vendor === "looker";
  const isCube = vendor === "cube";

  if (!showSection) {
    return (
      <div className='space-y-2'>
        <CardTitle>Semantic Engine</CardTitle>
        <Button type='button' variant='outline' size='sm' onClick={() => setShowSection(true)}>
          <Plus />
          Add Semantic Engine
        </Button>
        <p className='text-muted-foreground text-sm'>
          Optional vendor engine (Cube, Looker) for delegating query execution.
        </p>
      </div>
    );
  }

  return (
    <div className='space-y-4'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen} defaultOpen>
        <CollapsibleTrigger className='flex w-full items-center justify-between'>
          <div className='flex items-center gap-2'>
            {isOpen ? (
              <ChevronDown className='h-4 w-4 text-muted-foreground' />
            ) : (
              <ChevronRight className='h-4 w-4 text-muted-foreground' />
            )}
            <CardTitle>Semantic Engine</CardTitle>
          </div>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={(e) => {
              e.stopPropagation();
              setValue("semantic_engine", undefined, { shouldDirty: true });
              setShowSection(false);
            }}
          >
            <Trash2 className='h-4 w-4' />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className='mt-4 space-y-4 rounded-lg border p-4'>
          {/* vendor — required, auto-suggest */}
          <div className='space-y-2'>
            <Label>
              Vendor <span className='text-destructive'>*</span>
            </Label>
            <Controller
              name='semantic_engine.vendor'
              control={control}
              render={({ field }) => (
                <Select onValueChange={field.onChange} value={field.value ?? ""}>
                  <SelectTrigger>
                    <SelectValue placeholder='Select vendor' />
                  </SelectTrigger>
                  <SelectContent>
                    {SEMANTIC_ENGINE_VENDORS.map((opt) => (
                      <SelectItem className='cursor-pointer' key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            />
          </div>

          {/* base_url — required */}
          <div className='space-y-2'>
            <Label htmlFor='semantic_engine.base_url'>
              Base URL <span className='text-destructive'>*</span>
            </Label>
            <Input
              id='semantic_engine.base_url'
              placeholder={
                isLooker ? "e.g., https://myco.looker.com" : "e.g., https://cube.example.com"
              }
              {...register("semantic_engine.base_url")}
            />
          </div>

          {/* api_token — Cube */}
          {(isCube || !vendor) && (
            <div className='space-y-2'>
              <Label htmlFor='semantic_engine.api_token'>API Token</Label>
              <Input
                id='semantic_engine.api_token'
                placeholder='e.g., $&#123;CUBE_API_TOKEN&#125;'
                {...register("semantic_engine.api_token")}
              />
              <p className='text-muted-foreground text-sm'>
                Required for Cube. Supports $&#123;ENV_VAR&#125; interpolation.
              </p>
            </div>
          )}

          {/* client_id / client_secret — Looker */}
          {(isLooker || !vendor) && (
            <>
              <div className='space-y-2'>
                <Label htmlFor='semantic_engine.client_id'>Client ID</Label>
                <Input
                  id='semantic_engine.client_id'
                  placeholder='e.g., $&#123;LOOKER_CLIENT_ID&#125;'
                  {...register("semantic_engine.client_id")}
                />
              </div>
              <div className='space-y-2'>
                <Label htmlFor='semantic_engine.client_secret'>Client Secret</Label>
                <Input
                  id='semantic_engine.client_secret'
                  placeholder='e.g., $&#123;LOOKER_CLIENT_SECRET&#125;'
                  {...register("semantic_engine.client_secret")}
                />
              </div>
            </>
          )}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
