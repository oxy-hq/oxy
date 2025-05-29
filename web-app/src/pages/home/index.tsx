import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";
import DataSection from "@/components/Database/DatabaseSection";
import { useSidebar } from "@/components/ui/shadcn/sidebar";

const Home = () => {
  const { open } = useSidebar();

  return (
    <div className="flex flex-col h-full">
      {!open && <PageHeader />}
      <div className="flex flex-col justify-center items-center h-full gap-9 px-4">
        <p className="text-4xl font-semibold">What can I help you with?</p>

        {/* Chat Panel - Center of screen */}
        <div className="flex justify-center w-full max-w-4xl">
          <ChatPanel />
        </div>
      </div>

      <div className="w-full px-4 pb-2">
        <div className="max-w-4xl mx-auto">
          <DataSection />
        </div>
      </div>
    </div>
  );
};

export default Home;
