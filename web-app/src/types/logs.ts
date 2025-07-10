export interface LogItem {
  id: string;
  user_id: string;
  prompts: string;
  thread_id: string;
  log: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  thread?: {
    id: string;
    title: string;
  };
}

export interface LogsResponse {
  logs: LogItem[];
}
