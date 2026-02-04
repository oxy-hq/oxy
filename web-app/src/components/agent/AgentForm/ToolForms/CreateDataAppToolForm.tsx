import type React from "react";

interface CreateDataAppToolFormProps {
  index: number;
}

export const CreateDataAppToolForm: React.FC<CreateDataAppToolFormProps> = () => {
  return (
    <div className='space-y-4'>
      <p className='text-muted-foreground text-sm'>
        This tool allows agents to create data applications. Only name and description fields are
        required.
      </p>
    </div>
  );
};
