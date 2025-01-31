declare module "@tauri-apps/plugin-dialog" {
  export function open(options: {
    directory?: boolean;
    multiple?: boolean;
    title?: string;
  }): Promise<string | string[] | null>;
}
