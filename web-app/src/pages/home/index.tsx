import ChatPanel from "@/components/Chat/ChatPanel";
import { Agent } from "@/components/Chat/ChatPanel/AgentsDropdown";
import PageHeader from "@/components/PageHeader";
import { useState } from "react";

const Home = () => {
  const [agent, setAgent] = useState<Agent | null>(null);
  return (
    <div className="flex flex-col h-full">
      <PageHeader className="md:hidden" />
      <div className="flex flex-col justify-center items-center h-full gap-9 px-4">
        <p className="text-4xl font-semibold">What can I help you with?</p>
        <div className="flex flex-col gap-4 w-full">
          <ChatPanel agent={agent} onChangeAgent={setAgent} />
        </div>
      </div>
    </div>
  );
};

export default Home;
