export type LogType = "info" | "error" | "warning" | "success";

export type LogItem = {
  timestamp: string;
  content: string;
  log_type: LogType;
  children?: LogItem[];
  error?: LogItem[];
  append?: boolean;
  is_streaming?: boolean;
};
