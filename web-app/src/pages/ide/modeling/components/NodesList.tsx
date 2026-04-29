import { ChevronRight, FileCode2, Layers3, Search } from "lucide-react";
import type React from "react";
import { useState } from "react";
import useModelingNodes from "@/hooks/api/modeling/useModelingNodes";
import useModelingProjects from "@/hooks/api/modeling/useModelingProjects";
import { cn } from "@/libs/shadcn/utils";
import type { NodeSummary } from "@/types/modeling";

const nodeIcon = (resourceType: string) => {
  switch (resourceType) {
    case "model":
      return <FileCode2 className='h-3.5 w-3.5 shrink-0' />;
    default:
      return <ChevronRight className='h-3.5 w-3.5 shrink-0' />;
  }
};

interface NodesListProps {
  selectedProjectName: string | null;
  selectedNodeId: string | null;
  onSelectProject: (name: string) => void;
  onSelectNode: (node: NodeSummary) => void;
}

const ProjectNodes: React.FC<{
  projectName: string;
  selectedNodeId: string | null;
  onSelectNode: (node: NodeSummary) => void;
}> = ({ projectName, selectedNodeId, onSelectNode }) => {
  const [query, setQuery] = useState("");
  const { data: nodes, isLoading, error } = useModelingNodes(projectName);

  if (isLoading) {
    return <div className='px-4 py-1 text-muted-foreground text-xs'>Loading…</div>;
  }

  if (error) {
    return <div className='px-4 py-1 text-destructive text-xs'>Failed to load models.</div>;
  }

  const models = (nodes?.filter((n) => n.resource_type === "model") ?? []).sort((a, b) =>
    a.name.localeCompare(b.name)
  );
  const filteredModels = query
    ? models.filter((n) => n.name.toLowerCase().includes(query.toLowerCase()))
    : models;

  const groups = [{ label: "Models", nodes: filteredModels }].filter((g) => g.nodes.length > 0);

  return (
    <>
      <div className='flex items-center gap-1.5 border-b px-2 py-1'>
        <Search className='h-3 w-3 shrink-0 text-muted-foreground' />
        <input
          type='search'
          placeholder='Filter models…'
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className='w-full bg-transparent py-0.5 text-xs outline-none placeholder:text-muted-foreground'
        />
      </div>
      {groups.length === 0 ? (
        <div className='px-4 py-1 text-muted-foreground text-xs'>
          {query ? `No models match "${query}".` : "No models found."}
        </div>
      ) : (
        groups.map((group) => (
          <div key={group.label}>
            <p className='px-4 py-1 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
              {group.label}
            </p>
            {group.nodes.map((node) => (
              <button
                key={node.unique_id}
                type='button'
                onClick={() => onSelectNode(node)}
                className={cn(
                  "flex w-full items-center gap-2 px-4 py-1.5 text-left text-sm hover:bg-accent",
                  selectedNodeId === node.unique_id && "bg-accent text-accent-foreground"
                )}
              >
                {nodeIcon(node.resource_type)}
                <span className='truncate'>{node.name}</span>
                {node.materialization && (
                  <span className='ml-auto shrink-0 text-muted-foreground text-xs'>
                    {node.materialization}
                  </span>
                )}
              </button>
            ))}
          </div>
        ))
      )}
    </>
  );
};

const NodesList: React.FC<NodesListProps> = ({
  selectedProjectName,
  selectedNodeId,
  onSelectProject,
  onSelectNode
}) => {
  const { data: rawProjects, isLoading, error } = useModelingProjects();
  const projects = rawProjects
    ? [...rawProjects].sort((a, b) => a.name.localeCompare(b.name))
    : rawProjects;
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(new Set());

  const toggleProject = (projectKey: string) => {
    setExpandedProjects((prev) => {
      const next = new Set(prev);
      if (next.has(projectKey)) {
        next.delete(projectKey);
      } else {
        next.add(projectKey);
        onSelectProject(projectKey);
      }
      return next;
    });
  };

  if (isLoading) {
    return <div className='px-3 py-4 text-muted-foreground text-sm'>Loading projects…</div>;
  }

  if (error || !projects) {
    return (
      <div className='px-3 py-4 text-destructive text-sm'>Failed to load modeling projects.</div>
    );
  }

  if (projects.length === 0) {
    return (
      <div className='space-y-1 px-3 py-4 text-muted-foreground text-xs'>
        <p className='font-medium'>No modeling projects found.</p>
        <p>
          Add a modeling project under <code className='font-mono'>modeling/</code> in your Oxy
          project.
        </p>
      </div>
    );
  }

  return (
    <div className='overflow-y-auto'>
      {projects.map((proj) => {
        const projectKey = proj.folder_name || proj.name;
        const isExpanded = expandedProjects.has(projectKey);
        const isActive = selectedProjectName === projectKey;
        return (
          <div key={projectKey}>
            <button
              type='button'
              onClick={() => toggleProject(projectKey)}
              className={cn(
                "flex w-full items-center gap-2 border-b px-3 py-2 text-left font-medium text-sm hover:bg-accent",
                isActive && "bg-accent/60 text-accent-foreground"
              )}
            >
              <Layers3 className='h-3.5 w-3.5 shrink-0 text-primary' />
              <span className='truncate'>{proj.name}</span>
              <ChevronRight
                className={cn(
                  "ml-auto h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                  isExpanded && "rotate-90"
                )}
              />
            </button>
            {isExpanded && (
              <ProjectNodes
                projectName={projectKey}
                selectedNodeId={selectedNodeId}
                onSelectNode={(node) => {
                  onSelectProject(projectKey);
                  onSelectNode(node);
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
};

export default NodesList;
