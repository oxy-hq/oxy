import { Loader2 } from "lucide-react";

interface Props {
  children: React.ReactNode;
  actions?: React.ReactNode;
  title: string;
  loading?: boolean;
}

const PageWrapper = ({ children, title, actions, loading }: Props) => {
  return (
    <div className="p-4 space-y-6">
      <div className="pb-2 flex items-center border-b border-border justify-between">
        <h3 className="text-xl h-9 content-center">{title}</h3>

        {!loading && actions}
      </div>
      <div>
        {!loading ? (
          children
        ) : (
          <div className="flex items-center justify-center h-30">
            <Loader2 className="animate-spin h-6 w-6" />
          </div>
        )}
      </div>
    </div>
  );
};

export default PageWrapper;
