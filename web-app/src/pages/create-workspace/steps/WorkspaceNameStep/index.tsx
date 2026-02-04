import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import ROUTES from "@/libs/utils/routes";
import Header from "../Header";
import WorkspaceTypeSelector, { type WorkspaceType } from "./WorkspaceTypeSelector";

export interface WorkspaceFormData {
  name: string;
  type: WorkspaceType;
}

interface WorkspaceNameStepProps {
  initialData: WorkspaceFormData | null;
  onNext: (data: WorkspaceFormData) => void;
}

export default function WorkspaceNameStep({ initialData, onNext }: WorkspaceNameStepProps) {
  const {
    watch,
    register,
    formState: { errors, isValid },
    handleSubmit,
    setValue
  } = useForm<WorkspaceFormData>({
    defaultValues: initialData || {
      name: "",
      type: "new"
    }
  });

  const navigate = useNavigate();

  return (
    <form onSubmit={handleSubmit((data) => onNext(data))} className='space-y-8'>
      <div className='space-y-6'>
        <Header
          title='Name your workspace'
          description='Name your workspace for your team to join it; can be modified in your settings.'
        />

        <div>
          <Input
            id='name'
            placeholder='Oxygen Intelligence'
            {...register("name", {
              required: "Workspace name is required"
            })}
            autoFocus
          />
          {errors.name && <p className='mt-1 text-destructive text-sm'>{errors.name.message}</p>}
        </div>
      </div>
      <div className='mt-8'>
        <WorkspaceTypeSelector
          selectedType={(watch("type") as WorkspaceType) || "new"}
          onTypeChange={(type) => {
            setValue("type", type);
          }}
        />
      </div>

      <div className='mt-6 flex justify-between'>
        <Button variant='outline' type='button' onClick={() => navigate(ROUTES.WORKSPACE.ROOT)}>
          Cancel
        </Button>
        <Button disabled={!isValid} type='submit'>
          Next
        </Button>
      </div>
    </form>
  );
}
