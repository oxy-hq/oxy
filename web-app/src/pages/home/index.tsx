import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";

const Home = () => {
  return (
    <div className="flex flex-col h-full">
      <PageHeader className="md:hidden" />
      <div className="flex flex-col justify-center items-center h-full gap-9 px-4">
        <p className="text-4xl font-semibold">Start with a question</p>
        <ChatPanel />
      </div>
    </div>
  );
};

export default Home;
