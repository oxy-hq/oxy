import type React from "react";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { SidebarMenuSub } from "@/components/ui/shadcn/sidebar";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { DatabaseSchema } from "@/types/database";
import { SchemaTableList } from "./SchemaTableList";

interface ConnectionSchemaContentProps {
  databaseName: string;
  dialect: string;
  schema: DatabaseSchema | undefined;
  isLoading: boolean;
  isError: boolean;
  onRefresh: (e: React.MouseEvent) => void;
}

export const ConnectionSchemaContent: React.FC<ConnectionSchemaContentProps> = ({
  databaseName,
  dialect,
  schema,
  isLoading,
  isError,
  onRefresh
}) => {
  if (isLoading) {
    return (
      <SidebarMenuSub className='ml-[15px]'>
        <div className='flex items-center gap-2 px-2 py-2 text-muted-foreground text-xs'>
          <Spinner className='size-2.5' />
          Fetching schema…
        </div>
      </SidebarMenuSub>
    );
  }

  if (isError) {
    return (
      <SidebarMenuSub className='ml-[15px]'>
        <div className='px-2 py-2 text-xs'>
          <ErrorAlert message='Failed to load schema' className='mb-1' />
          <button onClick={onRefresh} className='text-primary hover:underline'>
            Retry
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  if (!schema || schema.tables.length === 0) {
    return (
      <SidebarMenuSub className='ml-[15px]'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>No tables found</div>
      </SidebarMenuSub>
    );
  }

  return (
    <SidebarMenuSub className='ml-[15px]'>
      <SchemaTableList tables={schema.tables} dialect={dialect} databaseName={databaseName} />
    </SidebarMenuSub>
  );
};
