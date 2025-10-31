import React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { X } from "lucide-react";

interface HeaderProps {
  toggleOutput: () => void;
}

const Header: React.FC<HeaderProps> = ({ toggleOutput }) => {
  return (
    <div className="px-2 py-1 border border-border flex justify-between items-center bg-card">
      <span className="text-background-foreground text-sm">Output</span>
      <Button variant="ghost" content="icon" onClick={toggleOutput}>
        <X size={14} />
      </Button>
    </div>
  );
};

export default Header;
