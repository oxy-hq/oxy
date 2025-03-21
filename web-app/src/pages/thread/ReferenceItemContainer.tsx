type ReferenceItemContainerProps = {
  children: React.ReactNode;
};

export const ReferenceItemContainer = ({
  children,
}: ReferenceItemContainerProps) => {
  return (
    <div className="bg-sidebar-accent hover:bg-input border rounded-md">
      {children}
    </div>
  );
};
