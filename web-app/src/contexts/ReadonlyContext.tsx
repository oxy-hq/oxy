import React, { createContext } from "react";
import { useProjectStatus } from "@/hooks/useProjectStatus";

interface ReadonlyContextType {
  isReadonly: boolean;
  isLoading: boolean;
}

export const ReadonlyContext = createContext<ReadonlyContextType | undefined>(
  undefined,
);

export const ReadonlyProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const { data: projectStatus, isPending } = useProjectStatus();

  const isReadonly = projectStatus?.is_readonly ?? false;

  return (
    <ReadonlyContext.Provider value={{ isReadonly, isLoading: isPending }}>
      {children}
    </ReadonlyContext.Provider>
  );
};
