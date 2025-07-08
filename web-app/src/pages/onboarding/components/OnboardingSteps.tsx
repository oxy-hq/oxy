import {
  Check,
  GitBranch,
  Key,
  Settings,
  Shield,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { useState, useEffect } from "react";

export type StepStatus = "pending" | "active" | "completed";

export interface OnboardingStep {
  id: string;
  title: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  status: StepStatus;
  content?: React.ReactNode; // Actual step content component
}

interface OnboardingStepsProps {
  currentStep: "token" | "repository" | "syncing" | "secrets" | "complete";
  stepContents?: {
    token?: React.ReactNode;
    repository?: React.ReactNode;
    syncing?: React.ReactNode;
    secrets?: React.ReactNode;
    complete?: React.ReactNode;
  };
}

export const OnboardingSteps = ({
  currentStep,
  stepContents,
}: OnboardingStepsProps) => {
  const [expandedStep, setExpandedStep] = useState<string | null>(currentStep);

  // Update expanded step when current step changes
  useEffect(() => {
    setExpandedStep(currentStep);
  }, [currentStep]);

  const handleStepClick = (
    stepId: string,
    hasContent: boolean,
    status: StepStatus,
  ) => {
    if (!hasContent) return;

    // Only allow interaction with completed and active steps
    if (status === "pending") return;

    // Toggle expansion
    setExpandedStep(expandedStep === stepId ? null : stepId);
  };

  const getStepStatus = (stepId: string): StepStatus => {
    const stepOrder = ["token", "repository", "syncing", "secrets", "complete"];
    const currentIndex = stepOrder.indexOf(currentStep);
    const stepIndex = stepOrder.indexOf(stepId);

    if (stepIndex < currentIndex) return "completed";
    if (stepIndex === currentIndex) return "active";
    return "pending";
  };

  const steps: OnboardingStep[] = [
    {
      id: "token",
      title: "Connect GitHub",
      description: "Authenticate with GitHub using a personal access token",
      icon: GitBranch,
      status: getStepStatus("token"),
      content: stepContents?.token,
    },
    {
      id: "repository",
      title: "Repository Setup",
      description: "Select and configure your repository",
      icon: Settings,
      status: getStepStatus("repository"),
      content: stepContents?.repository,
    },
    {
      id: "syncing",
      title: "Clone Repository",
      description: "Download and sync repository files",
      icon: Shield,
      status: getStepStatus("syncing"),
      content: stepContents?.syncing,
    },
    {
      id: "secrets",
      title: "Configure Secrets",
      description: "Set up required environment variables",
      icon: Key,
      status: getStepStatus("secrets"),
      content: stepContents?.secrets,
    },
    {
      id: "complete",
      title: "Setup Complete",
      description: "Ready to start using Oxy",
      icon: Check,
      status: getStepStatus("complete"),
      content: stepContents?.complete,
    },
  ];

  const getStatusIcon = (status: StepStatus) => {
    switch (status) {
      case "completed":
        return <Check className="w-5 h-5 text-green-600" />;
      case "active":
        return (
          <div className="w-5 h-5 rounded-full bg-primary flex items-center justify-center">
            <div className="w-2 h-2 rounded-full bg-white"></div>
          </div>
        );
      case "pending":
        return (
          <div className="w-5 h-5 rounded-full border-2 border-gray-300"></div>
        );
    }
  };

  const getStepClasses = (status: StepStatus) => {
    switch (status) {
      case "completed":
        return "text-green-600 border-green-200 bg-green-50 hover:bg-green-100";
      case "active":
        return "text-white border-primary bg-primary/5 hover:bg-primary/10";
      case "pending":
        return "text-gray-400 border-gray-200 bg-gray-50 cursor-not-allowed";
    }
  };

  return (
    <div className="mb-8">
      <div className="max-w-2xl mx-auto">
        {steps.map((step) => {
          const Icon = step.icon;
          const isExpanded = expandedStep === step.id;
          const hasContent = Boolean(step.content);
          const isDisabled = step.status === "pending";

          return (
            <div key={step.id} className="relative">
              {/* Step content */}
              <div
                className={`border rounded-lg p-4 mb-3 transition-all duration-200 ${
                  hasContent && !isDisabled
                    ? "cursor-pointer hover:shadow-md"
                    : ""
                } ${isDisabled ? "opacity-60" : ""} ${getStepClasses(step.status)} ${
                  isExpanded ? "ring-2 ring-blue-200" : ""
                }`}
              >
                <div
                  className="flex items-start space-x-3"
                  onClick={() =>
                    handleStepClick(step.id, hasContent, step.status)
                  }
                >
                  <div className="flex-shrink-0 mt-0.5">
                    {getStatusIcon(step.status)}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center space-x-2">
                      <Icon className="w-4 h-4" />
                      <h3 className="font-medium text-sm">{step.title}</h3>
                      <div className="ml-auto">
                        {hasContent &&
                          !isDisabled &&
                          (isExpanded ? (
                            <ChevronDown className="w-4 h-4 text-gray-500" />
                          ) : (
                            <ChevronRight className="w-4 h-4 text-gray-500" />
                          ))}
                      </div>
                    </div>
                    <p className="text-xs mt-1 opacity-80">
                      {step.description}
                    </p>
                  </div>
                  {step.status === "active" && (
                    <div className="flex-shrink-0">
                      <div className="animate-pulse w-2 h-2 bg-primary rounded-full"></div>
                    </div>
                  )}
                </div>

                {/* Expanded content */}
                {isExpanded && step.content && (
                  <div className="mt-4 pt-4 border-t border-gray-200">
                    {step.content}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
