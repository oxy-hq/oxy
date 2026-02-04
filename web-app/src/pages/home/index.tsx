import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

const Home = () => {
  const { open } = useSidebar();

  return (
    <div className='flex h-full flex-col'>
      {!open && <PageHeader />}
      <div className='flex h-full flex-col items-center justify-center gap-9 px-4'>
        <p className='font-semibold text-4xl'>What can I help you with?</p>

        {/* Chat Panel - Center of screen */}
        <div className='flex w-full max-w-4xl justify-center'>
          <ChatPanel />
        </div>
      </div>
    </div>
  );
};

export default Home;
