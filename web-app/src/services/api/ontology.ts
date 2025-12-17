import {
  OntologyGraph,
  OntologyNode,
  OntologyEdge,
  View,
  Topic,
} from "@/types/ontology";
import { DatabaseService } from "./database";
import { FileService } from "./files";
import { FileTreeModel } from "@/types/file";
import { parse } from "yaml";

export class OntologyService {
  /**
   * Builds the complete ontology graph by fetching all assets and computing linkages
   */
  static async getOntologyGraph(
    projectId: string,
    branchName: string,
  ): Promise<OntologyGraph> {
    // Fetch all necessary data in parallel
    const [databases, fileTree] = await Promise.all([
      DatabaseService.listDatabases(projectId, branchName),
      FileService.getFileTree(projectId, branchName),
    ]);

    // Parse semantic models, agents, queries, workflows, and apps from file tree
    const { views, topics, agents, sqlQueries, workflows } =
      await this.parseProjectFiles(projectId, branchName, fileTree);

    // Build nodes
    const nodes: OntologyNode[] = [];
    const edges: OntologyEdge[] = [];

    // Add table nodes
    databases.forEach((db) => {
      Object.entries(db.datasets).forEach(([dataset, tables]) => {
        tables.forEach((table) => {
          const tableId = `table:${db.name}.${dataset}.${table}`;
          nodes.push({
            id: tableId,
            type: "table",
            label: table,
            data: {
              name: table,
              database: db.name,
              description: `Table in ${db.name}.${dataset}`,
              metadata: {
                dataset,
                dialect: db.dialect,
              },
            },
          });
        });
      });
    });

    // Add view nodes and link to tables
    views.forEach((view) => {
      const viewId = `view:${view.path}`;
      nodes.push({
        id: viewId,
        type: "view",
        label: view.name,
        data: {
          name: view.name,
          path: view.path,
          description: view.description,
          datasource: view.datasource,
          metadata: {
            dimensions: view.dimensions?.length || 0,
            measures: view.measures?.length || 0,
          },
        },
      });

      // Link view to its source table
      // Try to find the table node by matching datasource and table name
      const tableNode = nodes.find(
        (n) =>
          n.type === "table" &&
          n.data.database === view.datasource &&
          n.data.name === view.table,
      );

      if (tableNode) {
        edges.push({
          id: `${viewId}->${tableNode.id}`,
          source: viewId,
          target: tableNode.id,
          label: "uses",
          type: "uses",
        });
      }
    });

    // Track entities globally (deduplicated by name)
    const entityMap = new Map<string, string>(); // entityName -> entityId

    // Add entity nodes from views
    views.forEach((view) => {
      const viewId = `view:${view.path}`;

      // Parse entities from the view's entities array
      if (view.entities && Array.isArray(view.entities)) {
        view.entities.forEach((entity) => {
          const entityName = entity.name;
          const entityId = `entity:${entityName}`;

          // Create or reference entity node (deduplicated by name)
          if (!entityMap.has(entityName)) {
            entityMap.set(entityName, entityId);
            nodes.push({
              id: entityId,
              type: "entity",
              label: entityName,
              data: {
                name: entityName,
                metadata: {
                  type: entity.type,
                  description: entity.description,
                  keys: entity.keys,
                },
              },
            });
          }

          // Link entity to view
          edges.push({
            id: `${entityId}->${viewId}`,
            source: entityId,
            target: viewId,
            label: "defined in",
            type: "uses",
          });
        });
      }
    });

    // Add topic nodes and link to views
    topics.forEach((topic) => {
      const topicId = `topic:${topic.path}`;
      nodes.push({
        id: topicId,
        type: "topic",
        label: topic.name,
        data: {
          name: topic.name,
          path: topic.path,
          description: topic.description,
          metadata: {
            views: topic.views,
            base_view: topic.base_view,
          },
        },
      });

      // Link topic to its views
      topic.views.forEach((viewName) => {
        const viewNode = nodes.find(
          (n) => n.type === "view" && n.data.name === viewName,
        );
        if (viewNode) {
          edges.push({
            id: `${topicId}->${viewNode.id}`,
            source: topicId,
            target: viewNode.id,
            label: "contains",
            type: "contains",
          });
        }
      });
    });

    // Add workflow, app, and automation nodes and link to their dependencies
    workflows.forEach((workflow) => {
      const isApp = workflow.path.endsWith(".app.yml");
      const isAutomation = workflow.path.endsWith(".automation.yml");

      let nodeId: string;
      let nodeType: "workflow" | "app" | "automation";

      if (isApp) {
        nodeId = `app:${workflow.path}`;
        nodeType = "app";
      } else if (isAutomation) {
        nodeId = `automation:${workflow.path}`;
        nodeType = "automation";
      } else {
        nodeId = `workflow:${workflow.path}`;
        nodeType = "workflow";
      }

      nodes.push({
        id: nodeId,
        type: nodeType,
        label: workflow.name,
        data: {
          name: workflow.name,
          path: workflow.path,
          description: workflow.description,
          metadata: {
            tasks: workflow.tasks,
          },
        },
      });

      // Analyze workflow/app tasks to find dependencies
      this.extractWorkflowDependencies(workflow.tasks, nodeId, nodes, edges);
    });

    // Helper function to check if a path matches a pattern (with wildcards)
    const matchesPattern = (path: string, pattern: string): boolean => {
      if (path === pattern) return true;
      if (!pattern.includes("*")) return false;
      const regexPattern = pattern.replace(/\*/g, ".*");
      return new RegExp(`^${regexPattern}$`).test(path);
    };

    // Build set of referenced SQL queries
    const referencedSqlQueries = new Set<string>();

    // Helper to check tasks for SQL query references
    const checkTasksForSqlReferences = (tasks: unknown[]): void => {
      if (!Array.isArray(tasks)) return;
      tasks.forEach((task) => {
        if (typeof task !== "object" || task === null) return;
        const taskObj = task as Record<string, unknown>;

        if (
          taskObj.type === "execute_sql" &&
          typeof taskObj.sql_file === "string"
        ) {
          sqlQueries.forEach((query) => {
            if (matchesPattern(query.path, taskObj.sql_file as string)) {
              referencedSqlQueries.add(query.path);
            }
          });
        }

        // Recursively check nested tasks
        if (Array.isArray(taskObj.tasks)) {
          checkTasksForSqlReferences(taskObj.tasks);
        }
      });
    };

    // Check workflows for execute_sql tasks
    workflows.forEach((workflow) => {
      checkTasksForSqlReferences(workflow.tasks);
    });

    // Helper to add SQL queries matching a pattern
    const addMatchingSqlQueries = (pattern: string) => {
      if (!pattern.endsWith(".sql")) return;
      sqlQueries.forEach((query) => {
        if (matchesPattern(query.path, pattern)) {
          referencedSqlQueries.add(query.path);
        }
      });
    };

    // Helper to extract SQL paths from context item
    const extractSqlPathsFromContext = (
      contextItem: string | { name?: string; type?: string; src?: string[] },
    ): string[] => {
      if (typeof contextItem === "string") {
        return [contextItem];
      }
      if (
        contextItem &&
        typeof contextItem === "object" &&
        Array.isArray(contextItem.src)
      ) {
        return contextItem.src.filter(
          (srcPath): srcPath is string => typeof srcPath === "string",
        );
      }
      return [];
    };

    // Check agents for SQL references in context and routes
    agents.forEach((agent) => {
      // Check context field
      if (agent.context) {
        agent.context.forEach((contextItem) => {
          const paths = extractSqlPathsFromContext(contextItem);
          paths.forEach(addMatchingSqlQueries);
        });
      }

      // Check routes field (for routing agents)
      if (agent.routes) {
        agent.routes.forEach((route) => {
          if (typeof route === "string") {
            addMatchingSqlQueries(route);
          }
        });
      }
    });

    // Add only referenced SQL query nodes
    sqlQueries.forEach((query) => {
      if (referencedSqlQueries.has(query.path)) {
        const queryId = `sql_query:${query.path}`;
        nodes.push({
          id: queryId,
          type: "sql_query",
          label: query.name,
          data: {
            name: query.name,
            path: query.path,
            metadata: {},
          },
        });
      }
    });

    // Add agent nodes and link to their dependencies
    agents.forEach((agent) => {
      const agentId = `agent:${agent.path}`;
      nodes.push({
        id: agentId,
        type: "agent",
        label: agent.name,
        data: {
          name: agent.name,
          path: agent.path,
          description: agent.description,
          metadata: {
            type: agent.agentType,
            model: agent.model,
          },
        },
      });

      // Link agent to topics (for default agents with semantic_query tools)
      if (agent.tools) {
        agent.tools.forEach((tool) => {
          if (tool.type === "semantic_query" && tool.topic) {
            const topicNode = nodes.find(
              (n) => n.type === "topic" && n.data.name === tool.topic,
            );
            if (topicNode) {
              edges.push({
                id: `${agentId}->${topicNode.id}`,
                source: agentId,
                target: topicNode.id,
                label: "uses",
                type: "uses",
              });
            }
          }

          // Link agent to workflows
          if (tool.type === "workflow" && tool.workflow_ref) {
            const workflowNode = nodes.find(
              (n) => n.type === "workflow" && n.data.path === tool.workflow_ref,
            );
            if (workflowNode) {
              edges.push({
                id: `${agentId}->${workflowNode.id}`,
                source: agentId,
                target: workflowNode.id,
                label: "uses",
                type: "uses",
              });
            }
          }
        });
      }

      // Link routing agent to topics and workflows via routes
      if (agent.agentType === "routing" && agent.routes) {
        agent.routes.forEach((route) => {
          if (typeof route !== "string") return;

          // Handle topic routes
          if (route.endsWith(".topic.yml") || route.endsWith(".topic.yaml")) {
            const matchedTopics = nodes.filter(
              (n) =>
                n.type === "topic" &&
                n.data.path &&
                matchesPattern(n.data.path, route),
            );
            matchedTopics.forEach((topicNode) => {
              edges.push({
                id: `${agentId}->${topicNode.id}`,
                source: agentId,
                target: topicNode.id,
                label: "routes to",
                type: "uses",
              });
            });
          }

          // Handle workflow routes
          if (route.endsWith(".workflow.yml")) {
            const matchedWorkflows = nodes.filter(
              (n) =>
                n.type === "workflow" &&
                n.data.path &&
                matchesPattern(n.data.path, route),
            );
            matchedWorkflows.forEach((workflowNode) => {
              edges.push({
                id: `${agentId}->${workflowNode.id}`,
                source: agentId,
                target: workflowNode.id,
                label: "routes to",
                type: "uses",
              });
            });
          }

          // Handle SQL query routes
          if (route.endsWith(".sql")) {
            const matchedQueries = nodes.filter(
              (n) =>
                n.type === "sql_query" &&
                n.data.path &&
                matchesPattern(n.data.path, route),
            );
            matchedQueries.forEach((queryNode) => {
              edges.push({
                id: `${agentId}->${queryNode.id}`,
                source: agentId,
                target: queryNode.id,
                label: "routes to",
                type: "uses",
              });
            });
          }
        });
      }

      // Link routing agent to fallback agent
      if (agent.agentType === "routing" && agent.route_fallback) {
        const fallbackAgentNode = nodes.find(
          (n) => n.type === "agent" && n.data.path === agent.route_fallback,
        );
        if (fallbackAgentNode) {
          edges.push({
            id: `${agentId}->${fallbackAgentNode.id}`,
            source: agentId,
            target: fallbackAgentNode.id,
            label: "fallback",
            type: "uses",
          });
        }
      }

      // Link agent to SQL queries via context
      if (agent.context) {
        const allPaths = agent.context.flatMap(extractSqlPathsFromContext);
        const sqlPaths = allPaths.filter((path) => path.endsWith(".sql"));

        sqlPaths.forEach((contextPath) => {
          const matchedQueries = nodes.filter(
            (n) =>
              n.type === "sql_query" &&
              n.data.path &&
              matchesPattern(n.data.path, contextPath),
          );
          matchedQueries.forEach((queryNode) => {
            edges.push({
              id: `${agentId}->${queryNode.id}`,
              source: agentId,
              target: queryNode.id,
              label: "uses",
              type: "uses",
            });
          });
        });
      }
    });

    return { nodes, edges };
  }

  /**
   * Parses project files (views, topics, agents, SQL queries, workflows, apps, automations) from the file tree
   */
  private static async parseProjectFiles(
    projectId: string,
    branchName: string,
    fileTree: FileTreeModel[],
  ): Promise<{
    views: View[];
    topics: Topic[];
    agents: Array<{
      name: string;
      path: string;
      description?: string;
      agentType?: string;
      model?: string;
      tools?: Array<{
        type: string;
        topic?: string;
        workflow_ref?: string;
      }>;
      routes?: string[];
      route_fallback?: string;
      context?: Array<
        string | { name?: string; type?: string; src?: string[] }
      >;
    }>;
    sqlQueries: Array<{
      name: string;
      path: string;
    }>;
    workflows: Array<{
      name: string;
      path: string;
      description?: string;
      tasks: unknown[];
    }>;
  }> {
    const views: View[] = [];
    const topics: Topic[] = [];
    const agents: Array<{
      name: string;
      path: string;
      description?: string;
      agentType?: string;
      model?: string;
      tools?: Array<{
        type: string;
        topic?: string;
        workflow_ref?: string;
      }>;
      routes?: string[];
      route_fallback?: string;
      context?: Array<
        string | { name?: string; type?: string; src?: string[] }
      >;
    }> = [];
    const sqlQueries: Array<{
      name: string;
      path: string;
    }> = [];
    const workflows: Array<{
      name: string;
      path: string;
      description?: string;
      tasks: unknown[];
    }> = [];

    // Find all relevant files
    const projectFiles = this.findProjectFiles(fileTree);

    // Fetch and parse each file
    await Promise.all(
      projectFiles.map(async (file) => {
        try {
          const content = await FileService.getFile(
            projectId,
            btoa(file.path),
            branchName,
          );

          if (
            file.type === "view" ||
            file.type === "topic" ||
            file.type === "agent" ||
            file.type === "workflow" ||
            file.type === "app" ||
            file.type === "automation"
          ) {
            const parsed = parse(content);

            if (file.type === "view") {
              views.push({
                name: parsed.name,
                path: file.path,
                description: parsed.description,
                datasource: parsed.datasource,
                table: parsed.table,
                entities: parsed.entities,
                dimensions: parsed.dimensions,
                measures: parsed.measures,
              });
            } else if (file.type === "topic") {
              topics.push({
                name: parsed.name,
                path: file.path,
                description: parsed.description,
                views: parsed.views || [],
                base_view: parsed.base_view,
                default_filters: parsed.default_filters,
              });
            } else if (file.type === "agent") {
              const agentName =
                file.path.split("/").pop()?.replace(".agent.yml", "") ||
                "unknown";
              agents.push({
                name: agentName,
                path: file.path,
                description: parsed.description,
                agentType: parsed.type || "default",
                model: parsed.model,
                tools: parsed.tools,
                routes: parsed.routes,
                route_fallback: parsed.route_fallback,
                context: parsed.context,
              });
            } else if (
              file.type === "workflow" ||
              file.type === "app" ||
              file.type === "automation"
            ) {
              const workflowName =
                parsed.name ||
                file.path
                  .split("/")
                  .pop()
                  ?.replace(/\.(workflow|app|automation)\.yml$/, "") ||
                "unknown";
              workflows.push({
                name: workflowName,
                path: file.path,
                description: parsed.description,
                tasks: parsed.tasks || [],
              });
            }
          } else if (file.type === "sql") {
            const queryName =
              file.path.split("/").pop()?.replace(".sql", "") || "unknown";
            sqlQueries.push({
              name: queryName,
              path: file.path,
            });
          }
        } catch (error) {
          console.error(`Failed to parse ${file.path}:`, error);
        }
      }),
    );

    return { views, topics, agents, sqlQueries, workflows };
  }

  /**
   * Recursively finds all project files (views, topics, agents, SQL, workflows, apps, automations) in the file tree
   */
  private static findProjectFiles(
    fileTree: FileTreeModel[],
    basePath = "",
  ): Array<{ path: string; type: string }> {
    const files: Array<{ path: string; type: string }> = [];

    fileTree.forEach((node) => {
      const currentPath = basePath ? `${basePath}/${node.name}` : node.name;

      if (node.is_dir && node.children) {
        files.push(...this.findProjectFiles(node.children, currentPath));
      } else if (!node.is_dir) {
        if (
          node.name.endsWith(".view.yml") ||
          node.name.endsWith(".view.yaml")
        ) {
          files.push({ path: currentPath, type: "view" });
        } else if (
          node.name.endsWith(".topic.yml") ||
          node.name.endsWith(".topic.yaml")
        ) {
          files.push({ path: currentPath, type: "topic" });
        } else if (
          node.name.endsWith(".agent.yml") ||
          node.name.endsWith(".agent.yaml")
        ) {
          files.push({ path: currentPath, type: "agent" });
        } else if (node.name.endsWith(".sql")) {
          files.push({ path: currentPath, type: "sql" });
        } else if (
          node.name.endsWith(".workflow.yml") ||
          node.name.endsWith(".workflow.yaml")
        ) {
          files.push({ path: currentPath, type: "workflow" });
        } else if (
          node.name.endsWith(".app.yml") ||
          node.name.endsWith(".app.yaml")
        ) {
          files.push({ path: currentPath, type: "app" });
        } else if (
          node.name.endsWith(".automation.yml") ||
          node.name.endsWith(".automation.yaml")
        ) {
          files.push({ path: currentPath, type: "automation" });
        }
      }
    });

    return files;
  }

  /**
   * Extracts dependencies from workflow tasks
   */
  private static extractWorkflowDependencies(
    tasks: unknown[],
    workflowId: string,
    nodes: OntologyNode[],
    edges: OntologyEdge[],
  ): void {
    if (!Array.isArray(tasks)) return;

    // eslint-disable-next-line sonarjs/cognitive-complexity
    tasks.forEach((task) => {
      // Type guard to ensure task is an object with properties
      if (typeof task !== "object" || task === null) return;

      const taskObj = task as Record<string, unknown>;

      // Link to semantic queries (topics)
      if (
        taskObj.type === "semantic_query" &&
        typeof taskObj.topic === "string"
      ) {
        const topicNode = nodes.find(
          (n) => n.type === "topic" && n.data.name === taskObj.topic,
        );
        if (topicNode) {
          edges.push({
            id: `${workflowId}->${topicNode.id}`,
            source: workflowId,
            target: topicNode.id,
            label: "uses",
            type: "uses",
          });
        }

        // Also link to views referenced in dimensions/measures
        const viewNames = new Set<string>();

        // Extract view names from dimensions
        if (Array.isArray(taskObj.dimensions)) {
          taskObj.dimensions.forEach((dim) => {
            if (typeof dim === "string" && dim.includes(".")) {
              const viewName = dim.split(".")[0];
              viewNames.add(viewName);
            }
          });
        }

        // Extract view names from measures
        if (Array.isArray(taskObj.measures)) {
          taskObj.measures.forEach((measure) => {
            if (typeof measure === "string" && measure.includes(".")) {
              const viewName = measure.split(".")[0];
              viewNames.add(viewName);
            }
          });
        }

        // Link to views
        viewNames.forEach((viewName) => {
          const viewNode = nodes.find(
            (n) => n.type === "view" && n.data.name === viewName,
          );
          if (viewNode) {
            edges.push({
              id: `${workflowId}->${viewNode.id}`,
              source: workflowId,
              target: viewNode.id,
              label: "queries",
              type: "uses",
            });
          }
        });
      }

      // Link to SQL query files
      if (
        taskObj.type === "execute_sql" &&
        typeof taskObj.sql_file === "string"
      ) {
        const queryNode = nodes.find(
          (n) => n.type === "sql_query" && n.data.path === taskObj.sql_file,
        );
        if (queryNode) {
          edges.push({
            id: `${workflowId}->${queryNode.id}`,
            source: workflowId,
            target: queryNode.id,
            label: "uses",
            type: "uses",
          });
        }
      }

      // Link to agents
      if (taskObj.type === "agent" && typeof taskObj.agent_ref === "string") {
        const agentNode = nodes.find(
          (n) => n.type === "agent" && n.data.path === taskObj.agent_ref,
        );
        if (agentNode) {
          edges.push({
            id: `${workflowId}->${agentNode.id}`,
            source: workflowId,
            target: agentNode.id,
            label: "uses",
            type: "uses",
          });
        }
      }

      // Link to other workflows
      if (
        taskObj.type === "workflow" &&
        typeof taskObj.workflow_ref === "string"
      ) {
        const subWorkflowNode = nodes.find(
          (n) => n.type === "workflow" && n.data.path === taskObj.workflow_ref,
        );
        if (subWorkflowNode) {
          edges.push({
            id: `${workflowId}->${subWorkflowNode.id}`,
            source: workflowId,
            target: subWorkflowNode.id,
            label: "uses",
            type: "uses",
          });
        }
      }

      // Recursively handle nested tasks
      if (Array.isArray(taskObj.tasks)) {
        this.extractWorkflowDependencies(
          taskObj.tasks,
          workflowId,
          nodes,
          edges,
        );
      }
    });
  }
}
