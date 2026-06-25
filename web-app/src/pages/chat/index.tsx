import { askPlaceholder } from "@/components/Ask/askPlaceholder";
import ChatPanel from "@/components/Chat/ChatPanel";
import { ThreadHistory } from "@/components/ThreadHistory";
import { getGreeting } from "@/libs/utils/greeting";
import useCurrentOrg from "@/stores/useCurrentOrg";

/**
 * The Chat landing (rail "Chat" button) — a ChatGPT-like composer with the
 * workspace's recent threads below it. Reuses the same ask-locked composer as
 * the Ask dock (default agent, extended-thinking toggle, no selectors), but
 * WITHOUT `onThreadCreated`, so submitting navigates to the full thread page
 * (`/threads/:id`) rather than streaming in the dock.
 */
const ChatPage = () => {
  const orgName = useCurrentOrg((s) => s.org?.name);
  return (
    <div className='flex h-full flex-col overflow-auto'>
      <div className='mx-auto w-full max-w-2xl px-6 pt-16 pb-16 sm:pt-24'>
        <p className='mb-6 text-balance text-center text-xl sm:text-2xl'>
          {getGreeting()}! How can I assist you?
        </p>
        <ChatPanel lockMode='ask' hideAgentPicker placeholderOverride={askPlaceholder(orgName)} />
        <ThreadHistory className='mt-10' />
      </div>
    </div>
  );
};

export default ChatPage;
