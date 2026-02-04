import { createContext, type ReactNode, useContext, useState } from "react";
import { FilesSubViewMode } from "./FilesSidebar/constants";

interface FilesContextType {
  filesSubViewMode: FilesSubViewMode;
  setFilesSubViewMode: (mode: FilesSubViewMode) => void;
}

const FilesContext = createContext<FilesContextType>({
  filesSubViewMode: FilesSubViewMode.OBJECTS,
  setFilesSubViewMode: () => {}
});

export function useFilesContext(): FilesContextType {
  const context = useContext(FilesContext);
  if (!context) {
    throw new Error("useFilesContext must be used within a FilesProvider");
  }
  return context as FilesContextType;
}

export const FilesProvider = ({ children }: { children: ReactNode }) => {
  const [filesSubViewMode, setFilesSubViewMode] = useState<FilesSubViewMode>(
    FilesSubViewMode.OBJECTS
  );

  return (
    <FilesContext.Provider value={{ filesSubViewMode, setFilesSubViewMode }}>
      {children}
    </FilesContext.Provider>
  );
};
