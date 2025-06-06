export type LogType = "info" | "error" | "warning" | "success";

export type LogItem = {
  timestamp: string;
  content: string;
  log_type: LogType;
};
