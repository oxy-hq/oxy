import { useState } from "react";

import { Dir } from ".";
import DirTree from "./DirTree";

const NodeTree = ({
  node,
  isLast,
  prefix = "",
}: {
  node: Dir;
  isLast?: boolean;
  prefix?: string;
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const toggleOpen = () => setIsOpen(!isOpen);
  const connector = isLast ? "└── " : "├── ";

  return (
    <li key={node.name}>
      {node.children && node.children.length > 0 ? (
        <button onClick={toggleOpen} style={{ cursor: "pointer" }}>
          {isOpen ? "[-] " : "[+] "} {connector} 📁 {node.name}
        </button>
      ) : (
        <span>
          {connector} 📄 {node.name}
        </span>
      )}
      {isOpen && node.children && (
        <DirTree
          nodes={node.children}
          prefix={prefix + (isLast ? "    " : "│   ")}
        />
      )}
    </li>
  );
};

export default NodeTree;
