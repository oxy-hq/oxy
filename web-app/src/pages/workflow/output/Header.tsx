import React from "react";
import { DynamicIcon } from "lucide-react/dynamic";
import { Button } from "@/components/ui/shadcn/button";

interface HeaderProps {
  showOutput: boolean;
  toggleOutput: () => void;
}

const Header: React.FC<HeaderProps> = ({ showOutput, toggleOutput }) => {
  return (
    <div className="px-2 py-1 border border-border flex justify-between items-center">
      {showOutput ? (
        <>
          <span className="text-background-foreground text-sm">Output</span>
          <Button variant="ghost" content="icon" onClick={toggleOutput}>
            <DynamicIcon name="x" size={14} />
          </Button>
        </>
      ) : (
        <Button variant="ghost" content="icon" size="sm" onClick={toggleOutput}>
          <DynamicIcon name="chevrons-left" />
        </Button>
      )}
    </div>
  );
};

export default Header;
