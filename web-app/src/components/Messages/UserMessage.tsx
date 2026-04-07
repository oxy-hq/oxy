import type React from "react";

interface UserMessageProps {
  content: string;
  createdAt?: string;
}

function getDisplayName(filePath: string) {
  const fileName = filePath.split("/").pop() ?? filePath;
  return fileName
    .replace(/\.(procedure|workflow|automation|agent|aw|app|view|topic)\.(yml|yaml)$/, "")
    .replace(/\.(yml|yaml)$/, "");
}

function renderWithMentions(content: string) {
  const parts = content.split(/(<[^>]+>)/g);
  return parts.map((part) => {
    const match = part.match(/^<(.+)>$/);
    if (match) {
      const filePath = match[1];
      const displayName = getDisplayName(filePath);
      return (
        <span
          key={filePath}
          className='inline-flex items-center rounded-md bg-orange-500/15 px-1.5 py-0.5 font-medium text-orange-400 text-xs'
        >
          @{displayName}
        </span>
      );
    }
    return part;
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
