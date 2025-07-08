import { useContext } from "react";
import { ReadonlyContext } from "@/contexts/ReadonlyContext";

export const useReadonly = () => {
  const context = useContext(ReadonlyContext);
  if (context === undefined) {
    throw new Error("useReadonly must be used within a ReadonlyProvider");
  }
  return context;
};
