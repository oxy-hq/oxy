import { useQuery } from "@tanstack/react-query";

import { apiClient } from "@/services/axios";

import SystemConfig from "./SystemConfig";

const SystemPage = () => {
  const { data: systemData, isLoading } = useQuery({
    queryKey: ["systemData"],
    queryFn: async () => {
      const response = await apiClient.get("/load-config");
      return response.data;
    }
  });

  return <div>{isLoading ? <p>Loading...</p> : <SystemConfig data={systemData} />}</div>;
};

export default SystemPage;

