export type WorkspaceType = "new" | "github";

interface WorkspaceTypeSelectorProps {
  selectedType?: WorkspaceType;
  onTypeChange?: (type: WorkspaceType) => void;
}

interface WorkspaceOptionProps {
  title: string;
  description: string;
  icon: React.ReactNode;
  isSelected: boolean;
  onSelect: () => void;
}

const WorkspaceOption = ({
  title,
  description,
  icon,
  isSelected,
  onSelect,
}: WorkspaceOptionProps) => (
  <div
    className={`p-4 border rounded-lg cursor-pointer transition-all ${
      isSelected ? "border-primary bg-primary/5" : "hover:border-primary/50"
    }`}
    onClick={onSelect}
  >
    <div className="flex items-center gap-2">
      <div className="p-2 rounded-full bg-primary/10">{icon}</div>
      <span className="font-medium">{title}</span>
    </div>
    <p className="text-sm text-muted-foreground mt-2">{description}</p>
  </div>
);

export default function WorkspaceTypeSelector({
  selectedType = "new",
  onTypeChange,
}: WorkspaceTypeSelectorProps) {
  const handleTypeChange = (type: WorkspaceType) => {
    if (onTypeChange) onTypeChange(type);
  };

  const workspaceOptions = [
    {
      type: "new" as const,
      title: "Create new workspace",
      description:
        "Create a new workspace from scratch with agent, warehouse, and model settings.",
      icon: (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          className="h-5 w-5 text-primary"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path
            fillRule="evenodd"
            d="M10 3a1 1 0 011 1v5h5a1 1 0 110 2h-5v5a1 1 0 11-2 0v-5H4a1 1 0 110-2h5V4a1 1 0 011-1z"
            clipRule="evenodd"
          />
        </svg>
      ),
    },
    {
      type: "github" as const,
      title: "Import from GitHub",
      description: "Create a workspace from an existing GitHub repository.",
      icon: (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          className="h-5 w-5 text-primary"
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
        </svg>
      ),
    },
  ];

  return (
    <div className="mt-8">
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {workspaceOptions.map((option) => (
          <WorkspaceOption
            key={option.type}
            title={option.title}
            description={option.description}
            icon={option.icon}
            isSelected={selectedType === option.type}
            onSelect={() => handleTypeChange(option.type)}
          />
        ))}
      </div>
    </div>
  );
}
