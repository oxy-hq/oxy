import { createContext, useContext, useState } from "react";

interface FileTreeContextType {
  focusedPath: string | null;
  setFocusedPath: (path: string | null) => void;
}

const FileTreeContext = createContext<FileTreeContextType | null>(null);

export function FileTreeProvider({ children }: { children: React.ReactNode }) {
  const [focusedPath, setFocusedPath] = useState<string | null>(null);

  return (
    <FileTreeContext.Provider value={{ focusedPath, setFocusedPath }}>
      {children}
    </FileTreeContext.Provider>
  );
}

export function useFileTree() {
  const context = useContext(FileTreeContext);
  if (!context) {
    throw new Error("useFileTree must be used within a FileTreeProvider");
  }
  return context;
}
