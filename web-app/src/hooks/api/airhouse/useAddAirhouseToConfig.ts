import { useMutation, useQueryClient } from "@tanstack/react-query";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { FileService } from "@/services/api";
import queryKeys from "../queryKey";

const CONFIG_PATH = "config.yml";
const CONFIG_PATHB64 = encodeBase64(CONFIG_PATH);

export type AddToConfigResult = "added" | "already_present";

export default function useAddAirhouseToConfig() {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation<AddToConfigResult, Error, { name: string }>({
    mutationFn: async ({ name }) => {
      const content = await FileService.getFile(project.id, CONFIG_PATHB64, branchName);
      const parsed = parseYaml(content);
      const config: Record<string, unknown> =
        parsed !== null && typeof parsed === "object" ? (parsed as Record<string, unknown>) : {};
      const databases: unknown[] = Array.isArray(config.databases) ? config.databases : [];

      const alreadyPresent = databases.some(
        (db) =>
          db !== null &&
          typeof db === "object" &&
          (db as Record<string, unknown>).type === "airhouse_managed"
      );
      if (alreadyPresent) return "already_present";

      databases.push({ name, type: "airhouse_managed" });
      config.databases = databases;

      await FileService.saveFile(project.id, CONFIG_PATHB64, stringifyYaml(config), branchName);
      return "added";
    },
    onSuccess: () => {
      queryClient.removeQueries({
        queryKey: queryKeys.file.get(project.id, branchName, CONFIG_PATHB64)
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.diffSummary(project.id, branchName)
      });
    }
  });
}
