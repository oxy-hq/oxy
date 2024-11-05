import { Dir } from ".";
import NodeTree from "./NodeTree";

const DirTree = ({ nodes, prefix = "" }: { nodes: Dir[]; prefix?: string }) => {
  const sortedNodes = [...nodes].sort((a, b) => {
    const aIsDir = a.type === "dir";
    const bIsDir = b.type === "dir";

    const sortOrder = aIsDir ? -1 : 1;
    return aIsDir === bIsDir ? 0 : sortOrder;
  });
  
  return (
    <ul style={{ listStyleType: "none", paddingLeft: "20px" }}>
      {sortedNodes.map((node: Dir, index) => {
        const isLast = index === nodes.length - 1;

        return <NodeTree key={node.name + index} isLast={isLast} prefix={prefix} node={node} />;
      })}
    </ul>
  );
};

export default DirTree;

