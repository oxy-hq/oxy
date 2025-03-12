export type Conversation = {
  id: string;
  title: string;
  agent: string;
};

export type Agent = {
  description: string;
  updated_at: Date;
  path: string;
};

export type Message = {
  id: string;
  created_at?: Date;
  content: string;
  is_human: boolean;
};

export type Answer = {
  content: string;
  is_error: boolean;
};

export type ChatRequest = {
  agent: string;
  question: string;
  title: string;
  memory: Message[];
  project_path: string;
};

export type ThreadItem = {
  id: string;
  title: string;
  question: string;
  answer: string;
  agent: string;
  created_at: string;
};

export type ThreadCreateRequest = {
  title: string;
  question: string;
  agent: string;
};

export type ChatType = "chat" | "preview";
