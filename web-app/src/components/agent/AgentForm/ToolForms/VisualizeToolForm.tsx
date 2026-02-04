import type React from "react";

interface VisualizeToolFormProps {
  index: number;
}

export const VisualizeToolForm: React.FC<VisualizeToolFormProps> = () => {
  return (
    <div className='space-y-4'>
      <p className='text-muted-foreground text-sm'>
        This tool allows agents to create visualizations and charts. Only name and description
        fields are required.
      </p>
    </div>
  );
};
