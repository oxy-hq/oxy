import { apiClient } from "./axios";

export type AppIntegrationKind = "toast" | "openweathermap" | "besttime" | "unifi";

export type AppIntegration =
  | {
      kind: "toast";
      name: string;
      webhook_secret_var: string;
      restaurant_guids: string[];
    }
  | {
      kind: "openweathermap";
      name: string;
      api_key_var: string;
    }
  | {
      kind: "besttime";
      name: string;
      api_key_var: string;
    }
  | {
      kind: "unifi";
      name: string;
      api_key_var: string;
    };

export type UpsertAppIntegrationBody =
  | {
      kind: "toast";
      name: string;
      webhook_secret_var: string;
      restaurant_guids: string[];
    }
  | {
      kind: "openweathermap";
      name: string;
      api_key_var: string;
    }
  | {
      kind: "besttime";
      name: string;
      api_key_var: string;
    }
  | {
      kind: "unifi";
      name: string;
      api_key_var: string;
    };

export const AppIntegrationsService = {
  async list(projectId: string, branchName: string): Promise<AppIntegration[]> {
    const response = await apiClient.get<AppIntegration[]>(`/${projectId}/app-integrations`, {
      params: { branch: branchName }
    });
    return response.data;
  },

  async upsert(
    projectId: string,
    branchName: string,
    body: UpsertAppIntegrationBody
  ): Promise<void> {
    await apiClient.post(`/${projectId}/app-integrations`, body, {
      params: { branch: branchName }
    });
  },

  async remove(projectId: string, branchName: string, kind: AppIntegrationKind): Promise<void> {
    await apiClient.delete(`/${projectId}/app-integrations/${kind}`, {
      params: { branch: branchName }
    });
  }
};
