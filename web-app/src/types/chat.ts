export type Answer = {
  content: string;
  is_error: boolean;
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
