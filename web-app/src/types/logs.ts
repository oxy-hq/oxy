export interface QueryItem {
  query?: string;
  is_verified?: boolean;
  source?: string;
  database?: string;
}

export interface LogData {
  queries?: QueryItem[];
  [key: string]: unknown;
}

export interface LogItem {
  id: string;
  user_id: string;
  prompts: string;
  thread_id: string;
  log: LogData;
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
