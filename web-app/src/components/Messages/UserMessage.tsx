import type React from "react";

interface UserMessageProps {
  content: string;
  createdAt?: string;
}

function MentionChip({ label }: { label: string }) {
  return (
    <span className='inline-flex items-center rounded-md bg-vis-orange/15 px-1.5 py-0.5 font-medium text-vis-orange text-xs'>
      @{label}
    </span>
  );
}

function renderWithMentions(content: string) {
  const parts = content.split(/(<@[^|>]+\|[^>]+>)/g);
  return parts.map((part, i) => {
    const match = part.match(/^<@([^|>]+)\|([^>]+)>$/);
    if (match) {
      return <MentionChip key={i} label={match[2]} />;
    }
    return <span key={i}>{part}</span>;
  });
}

const UserMessage: React.FC<UserMessageProps> = ({ content }) => {
  return (
    <div
      className='inline-block max-w-[80%] rounded-2xl bg-card px-4 py-2.5 text-foreground text-sm'
      data-testid='user-message-text'
    >
      {renderWithMentions(content)}
    </div>
  );
};

export default UserMessage;
