import ApiKeyManagement from "@/components/settings/api-keys";

export default function ApiKeysPage() {
  return (
    <div className='customScrollbar scrollbar-gutter-auto h-full min-h-0 overflow-auto'>
      <ApiKeyManagement />
    </div>
  );
}
