export type Conversation = {
  id: string;
  title: string;
  agent: string;
};

export type Agent = {
  name: string;
  description: string;
  updated_at: Date;
};

export type Message = {
  id: string;
  created_at?: Date;
  content: string;
  is_human: boolean;
};