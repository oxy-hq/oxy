import { useState } from "react";
import { useCreateWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import useTheme from "@/stores/useTheme";
import StepHeader, { Step } from "./components/StepHeader";

import {
  WorkspaceFormData,
  default as WorkspaceNameStep,
} from "./steps/WorkspaceNameStep/index";
import { default as WarehouseStep } from "./steps/WarehouseStep/index";
import { WarehousesFormData } from "./steps/WarehouseStep/index";
import { default as ModelStep } from "./steps/ModelStep/index";
import { ModelsFormData } from "./steps/ModelStep/index";
import { default as AgentStep } from "./steps/AgentStep/index";
import { AgentConfig } from "./steps/AgentStep";
import { default as GitHubImportStep } from "./steps/GitHubImportStep/index";
import { GitHubData } from "./steps/GitHubImportStep/index";
import { WorkspaceType } from "./steps/WorkspaceNameStep/WorkspaceTypeSelector";

export interface CreateWorkspaceState {
  workspace: WorkspaceFormData | null;
  warehouses: WarehousesFormData | null;
  model: ModelsFormData | null;
  agent: AgentConfig | null;
  github: GitHubData | null;
}

const newWorkspaceSteps: Step[] = [
  { id: "workspace", label: "Workspace" },
  { id: "warehouse", label: "Warehouse" },
  { id: "model", label: "Model" },
  { id: "agent", label: "Agent" },
];

const githubWorkspaceSteps: Step[] = [
  { id: "workspace", label: "Workspace" },
  { id: "github-import", label: "GitHub" },
];

export default function CreateWorkspacePage() {
  const [currentStep, setCurrentStep] = useState<string>("workspace");
  const { theme } = useTheme();

  const {
    isError,
    error,
    mutateAsync,
    isPending: isCreating,
  } = useCreateWorkspace();

  const [workspaceState, setWorkspaceState] = useState<CreateWorkspaceState>({
    workspace: {
      name: "",
      type: "new",
    },
    warehouses: null,
    model: null,
    agent: null,
    github: null,
  });

  const { type: workspaceType } = workspaceState.workspace || {};

  const getSteps = (type?: WorkspaceType) => {
    const workspaceType = type || workspaceState.workspace?.type;
    return workspaceType === "github"
      ? githubWorkspaceSteps
      : newWorkspaceSteps;
  };

  const updateWorkspaceState = (newState: Partial<CreateWorkspaceState>) => {
    setWorkspaceState((prev) => ({
      ...prev,
      ...newState,
    }));
  };

  const handleNext = (type?: WorkspaceType) => {
    const steps = getSteps(type);
    const currentIndex = steps.findIndex((step) => step.id === currentStep);
    if (currentIndex < steps.length - 1) {
      setCurrentStep(steps[currentIndex + 1].id);
    }
  };

  const handleBack = () => {
    const steps = getSteps();
    const currentIndex = steps.findIndex((step) => step.id === currentStep);
    if (currentIndex > 0) {
      setCurrentStep(steps[currentIndex - 1].id);
    }
  };

  return (
    <div className="flex flex-col w-full overflow-auto customScrollbar">
      <div className="p-4">
        <div className="flex items-center gap-2">
          <img
            width={24}
            height={24}
            src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
            alt="Oxy"
          />
        </div>
        {workspaceType && (
          <StepHeader steps={getSteps()} currentStep={currentStep} />
        )}
      </div>

      <div className="flex-1 p-6 max-w-6xl mx-auto max-w-[520px] w-full">
        {isError && (
          <div className="p-3 mb-6 bg-destructive/10 border border-destructive/20 rounded-md text-sm text-destructive">
            {error instanceof Error
              ? error.message
              : "An error occurred while creating the workspace"}
          </div>
        )}

        {currentStep === "workspace" && (
          <WorkspaceNameStep
            initialData={workspaceState.workspace}
            onNext={(data) => {
              updateWorkspaceState({ workspace: data });
              handleNext(data.type);
            }}
          />
        )}

        {workspaceType === "new" && currentStep === "warehouse" && (
          <WarehouseStep
            initialData={workspaceState.warehouses}
            onNext={(data: WarehousesFormData) => {
              updateWorkspaceState({ warehouses: data });
              handleNext();
            }}
            onBack={handleBack}
          />
        )}

        {workspaceType === "new" && currentStep === "model" && (
          <ModelStep
            initialData={workspaceState.model}
            onNext={(data) => {
              updateWorkspaceState({ model: data });
              handleNext();
            }}
            onBack={handleBack}
          />
        )}

        {workspaceType === "new" && currentStep === "agent" && (
          <AgentStep
            initialData={workspaceState.agent}
            models={workspaceState.model}
            databases={workspaceState.warehouses}
            isCreating={isCreating}
            onNext={async (data) => {
              const updatedState = {
                ...workspaceState,
                agent: data,
              };

              updateWorkspaceState({ agent: data });

              try {
                await mutateAsync(updatedState);
                window.location.href = "/";
              } catch (err) {
                console.error("Failed to create workspace:", err);
              }
            }}
            onBack={handleBack}
          />
        )}

        {workspaceType === "github" && currentStep === "github-import" && (
          <GitHubImportStep
            isCreating={isCreating}
            initialData={
              workspaceState.github ? workspaceState.github : undefined
            }
            onNext={async (data) => {
              const updatedState = {
                ...workspaceState,
                github: data,
              };

              updateWorkspaceState({ github: data });

              try {
                await mutateAsync(updatedState);
                window.location.href = "/";
              } catch (err) {
                console.error("Failed to create workspace:", err);
              }
            }}
            onBack={handleBack}
          />
        )}
      </div>
    </div>
  );
}
