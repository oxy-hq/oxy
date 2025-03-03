import { create } from "zustand";
import useProjectPath from "./useProjectPath";
import { readTextFile } from "@tauri-apps/plugin-fs";
import { parse } from "yaml";

export type Database = {
  name: string;
};

export type Config = {
  databases: Database[];
};

interface ConfigState {
  getConfig: () => Promise<Config>;
}

const useConfig = create<ConfigState>(() => ({
  getConfig: async () => {
    const projectPath = useProjectPath.getState().projectPath;
    const configPath = `${projectPath}/config.yml`;
    const configObj = parse(await readTextFile(configPath));
    return configObj;
  },
}));

export default useConfig;
