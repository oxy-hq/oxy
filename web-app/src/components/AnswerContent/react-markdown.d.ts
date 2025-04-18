import "react-markdown";

import React from "react";

declare module "react-markdown" {
  export type ExtendedComponents = Components & {
    chart?: React.ComponentType<{ chart_src: string }>;
    reference?: React.ComponentType<{ children: React.ReactNode }>;
  };
}
