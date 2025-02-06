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
