import type { Monaco } from "@monaco-editor/react";
import { configureMonacoYaml } from "monaco-yaml";
import { monacoGitHubDarkDefaultTheme } from "@/components/FileEditor/hooks/github-dark-theme";
import YamlWorker from "@/components/FileEditor/hooks/yaml.worker.js?worker";

type WindowWithMonaco = Window & {
  MonacoEnvironment?: {
    getWorker?: (workerId?: string, label?: string) => Worker | Promise<Worker>;
  };
};

export const configureMonacoEnvironment = () => {
  (window as WindowWithMonaco).MonacoEnvironment = {
    getWorker: (_workerId?: string, label?: string): Worker | Promise<Worker> => {
      switch (label) {
        case "yaml":
          return new YamlWorker();
        default:
          return new Worker(
            new URL("monaco-editor/esm/vs/editor/editor.worker.js", import.meta.url),
            { type: "module" }
          );
      }
    }
  };
};

export const configureMonaco = (monaco: Monaco) => {
  monaco.editor.defineTheme("github-dark", monacoGitHubDarkDefaultTheme);
  monaco.editor.setTheme("github-dark");

  configureMonacoYaml(monaco, {
    enableSchemaRequest: true,
    hover: true,
    completion: true,
    validate: true,
    format: true,
    schemas: [
      {
        fileMatch: ["**/*.app.yml", "**/*.app.yaml"],
        uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/app.json"
      },
      {
        fileMatch: ["**/*.agent.yml", "**/*.agent.yaml"],
        uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/agent.json"
      },
      {
        fileMatch: ["**/*.workflow.yml", "**/*.workflow.yaml"],
        uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/workflow.json"
      },
      {
        fileMatch: ["**/config.yml", "**/config.yaml"],
        uri: "https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/config.json"
      }
    ]
  });
};
