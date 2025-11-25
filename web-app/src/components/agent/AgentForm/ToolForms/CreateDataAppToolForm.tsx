import React from "react";

interface CreateDataAppToolFormProps {
  index: number;
}

export const CreateDataAppToolForm: React.FC<
  CreateDataAppToolFormProps
> = () => {
  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">
        This tool allows agents to create data applications. Only name and
        description fields are required.
      </p>
    </div>
  );
};
