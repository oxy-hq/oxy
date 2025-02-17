import { create } from "zustand";

import { NodeData } from "@/pages/worflow/StepNode";

export type LayoutedNode = {
  id: string;
  size: {
    width: number;
    height: number;
  };
  position: {
    x: number;
    y: number;
  };
  data: NodeData;
  parentId?: string;
  hidden?: boolean;
};

export type Node = {
  id: string;
  parentId?: string;
  name: string;
  type: string;
  size: {
    width: number;
    height: number;
  };
  hidden: boolean;
  data: NodeData;
  width: number;
  height: number;
  children: Node[];
};

export type Edge = {
  id: string;
  source: string;
  target: string;
};

interface DiagramState {
  nodes: Node[];
  edges: Edge[];
  layoutedNodes: LayoutedNode[];
  setNodes: (nodes: Node[]) => void;
  updateNode: (node: Node) => void;
  upsertNode: (node: Node) => void;
  setEdges: (edges: Edge[]) => void;
  setLayoutedNodes: (layoutedNodes: LayoutedNode[]) => void;
  getNode: (id: string) => Node | null;
  setNodeVisibility: (id: string[], visible: boolean) => void;
}

const useDiagram = create<DiagramState>((set, get) => ({
  nodes: [],
  edges: [],
  layoutedNodes: [],
  setNodes: (nodes: Node[]) => set({ nodes }),
  updateNode: (node) =>
    set((state) => {
      const index = state.nodes.findIndex((n) => n.id === node.id);
      const nodes = [...state.nodes];
      nodes[index] = node;
      return { ...state, nodes };
    }),
  upsertNode: (node) => {
    set((state) => {
      const index = state.nodes.findIndex((n) => n.id === node.id);
      if (index === -1) {
        return { nodes: [...state.nodes, node] };
      }
      const nodes = [...state.nodes];
      nodes[index] = node;
      return { ...state, nodes };
    });
  },
  setEdges: (edges: Edge[]) => set({ edges }),
  setLayoutedNodes(layoutedNodes) {
    set({ layoutedNodes });
  },
  getNode: (id: string): Node | null => {
    const findNode = (nodes: Node[], id: string): Node | null => {
      const node = nodes.find((n) => n.id === id);
      if (node) return node;
      for (const n of nodes) {
        const found = findNode(n.children, id);
        if (found) return found;
      }
      return null;
    };
    return findNode(get().nodes, id);
  },
  setNodeVisibility: (ids: string[], visible: boolean) => {
    set((state) => {
      // Create a Set for faster lookup of node IDs
      const nodeIds = new Set(ids);
      const newNodes = state.nodes.map((node) => {
        // Check if the node or its parent is in the Set
        if (nodeIds.has(node.id) || nodeIds.has(node.parentId!)) {
          // Add the node's ID to the Set to handle its children
          nodeIds.add(node.id);
          // Return a new node object with updated hidden property
          return { ...node, hidden: !visible };
        }
        // Return the node unchanged if it doesn't match the criteria
        return node;
      });

      // Return the updated state with the new nodes
      return { ...state, nodes: newNodes };
    });
  },
}));

export default useDiagram;
