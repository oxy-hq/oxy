import { useEffect, useState } from "react";
import { DirEntry, readDir } from "@tauri-apps/plugin-fs";
import useProjectPath from "@/stores/useProjectPath";
import DropdownField from "./DropdownField";

// look for file end with .agent.yml in the projectPath
// return list of relative paths
// if not found, return empty list
const listAgents = async (projectPath: string) => {
  const entries = await readDir(projectPath);
  const processEntries = async (entries: DirEntry[], path: string) => {
    let paths: string[] = [];
    for (const entry of entries) {
      const entryPath = path + "/" + entry.name;
      if (entry.isDirectory) {
        const children = await readDir(entryPath);
        const childRs = await processEntries(children, entryPath);
        paths = paths.concat(childRs);
      } else {
        if (entry.name.endsWith(".agent.yml")) {
          paths.push(entryPath);
        }
      }
    }
    return paths;
  }

  const rs = await processEntries(entries, projectPath);
  const rsRelative = rs.map((path: string) => path.replace(projectPath, "")).map((path: string) => path.replace(/^\//, ""));
  return rsRelative;
}

export const AgentRefSelect = ({ ...props }) => {
  const projectPath = useProjectPath((state) => state.projectPath);
  const [agents, setAgents] = useState<string[]>([]);
  useEffect(() => {
    listAgents(projectPath).then(agents => {
      return setAgents(agents);
    }).catch(() => {
      setAgents([]);
    });
  }, [projectPath]);
  const options = agents?.map(agent => ({ label: agent, value: agent })) || [];
  return <DropdownField options={options} {...props} label="Agent reference">
  </DropdownField>
}
