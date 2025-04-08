import ChatPanel from "@/components/Chat/ChatPanel";
import { Agent } from "@/components/Chat/ChatPanel/AgentsDropdown";
import PageHeader from "@/components/PageHeader";
import { Button } from "@/components/ui/shadcn/button";
import { useState } from "react";
import { useNavigate } from "react-router-dom";

const Home = () => {
  const [agent, setAgent] = useState<Agent | null>(null);
  const navigate = useNavigate();
  return (
    <div className="flex flex-col h-full">
      <PageHeader className="md:hidden" />
      <div className="flex flex-col justify-center items-center h-full gap-9 px-4">
        <p className="text-4xl font-semibold">Start with a question</p>
        <div className="flex flex-col gap-4 w-full">
          <ChatPanel agent={agent} onChangeAgent={setAgent} />
          {agent && (
            <Button
              className="mx-auto"
              variant="outline"
              onClick={() => {
                navigate(`/agents/${btoa(agent.id)}/tests`);
              }}
            >
              Run tests for this agent
            </Button>
          )}
        </div>
      </div>
    </div>
  );
};

export default Home;
