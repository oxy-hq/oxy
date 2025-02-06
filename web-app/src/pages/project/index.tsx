import React from "react";

import { useQuery } from "@tanstack/react-query";

import { apiClient } from "@/services/axios";

import DirTree from "./DirTree";

export interface Dir {
  type: string;
  name: string;
  children: Dir[];
  isOpen?: boolean;
}

const Project: React.FC = () => {
  const {
    data: dirTree,
    isLoading,
    error,
  } = useQuery({
    queryKey: ["projectDir"],
    queryFn: async () => {
      const response = await apiClient.get(
        "http://127.0.0.1:3001/load-project-structure",
      );
      return response.data;
    },
  });

  if (isLoading) return <p>Loading...</p>;
  if (error) return <p>Error fetching directory tree: {error.message}</p>;

  return (
    <div>
      <DirTree nodes={dirTree} />
    </div>
  );
};

export default Project;
