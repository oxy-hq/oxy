import "react-markdown";

import React from "react";

declare module "react-markdown" {
  export type ExtendedComponents = Components & {
    chart?: React.ComponentType<{ chart_src: string }>;
    artifact?: React.ComponentType<{
      kind: string;
      title: string;
      is_verified: string;
      children: React.ReactNode;
    }>;
    reference?: React.ComponentType<{ children: React.ReactNode }>;
    table_virtualized?: React.ComponentType<{ table_id: string }>;
  };
}
