import ChatPanel from "@/components/Chat/ChatPanel";
import PageHeader from "@/components/PageHeader";
import useSidebar from "@/components/ui/shadcn/sidebar-context";

const getGreeting = () => {
  const hour = new Date().getHours();
  if (hour < 12) return "Good Morning";
  if (hour < 18) return "Good Afternoon";
  return "Good Evening";
};

const Home = () => {
  const { open } = useSidebar();

  const greeting = getGreeting();

  return (
    <div className='flex h-full flex-col'>
      {!open && <PageHeader />}
      <div className='flex h-full flex-col items-center justify-center gap-10 px-4'>
        <div className='flex max-w-2xl flex-col items-center justify-center'>
          <video autoPlay muted loop className='h-40 w-40 opacity-70'>
            <source src='https://www.oxy.tech/oxy_webm_final.webm' type='video/webm' />
          </video>
          <p className='text-3xl'>{greeting}! How can I assist you?</p>
        </div>

        {/* Chat Panel - Center of screen */}
        <div className='flex w-full max-w-4xl justify-center pb-40'>
          <ChatPanel />
        </div>
      </div>
    </div>
  );
};

export default Home;
