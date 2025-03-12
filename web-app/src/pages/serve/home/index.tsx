import ChatPanel from "@/components/serve/Chat/ChatPanel";

const Home = () => {
  return (
    <div className="flex flex-col justify-center items-center h-full gap-9 px-4">
      <p className="text-4xl font-semibold">Start with a question</p>
      <ChatPanel />
    </div>
  );
};

export default Home;
