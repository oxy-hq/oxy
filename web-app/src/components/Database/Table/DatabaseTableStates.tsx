import React from "react";
import { Loader2, Database } from "lucide-react";

export const DatabaseTableLoading: React.FC = () => (
  <div className="flex items-center justify-center p-8">
    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
    <span className="ml-2 text-muted-foreground">Loading databases...</span>
  </div>
);

export const DatabaseTableEmpty: React.FC = () => (
  <div className="flex flex-col items-center justify-center p-8 text-center">
    <Database className="h-12 w-12 text-muted-foreground mb-4" />
    <h3 className="text-lg font-semibold mb-2">No databases configured</h3>
    <p className="text-muted-foreground">
      Configure your database connections to get started with data management.
    </p>
  </div>
);
