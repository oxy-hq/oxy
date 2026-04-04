import EmptyState from "@/components/ui/EmptyState";

const AgenticAnalyticsTests = () => {
  return (
    <EmptyState
      className='h-full'
      title='No tests configured'
      description='Tests for analytics agents are not yet supported.'
    />
  );
};

export default AgenticAnalyticsTests;
