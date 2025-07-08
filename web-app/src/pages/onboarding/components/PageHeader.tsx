interface PageHeaderProps {
  title: string;
}

export const PageHeader = ({ title }: PageHeaderProps) => {
  return (
    <div className="flex items-center justify-center mb-6">
      <div>
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white">
          {title}
        </h1>
      </div>
    </div>
  );
};
