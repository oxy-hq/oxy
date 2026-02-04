import { SidebarMenuItem, SidebarMenuSkeleton } from "@/components/ui/shadcn/sidebar";

const ItemsSkeleton = () => {
  return Array.from({ length: 5 }).map((_, index) => (
    <SidebarMenuItem key={index}>
      <SidebarMenuSkeleton />
    </SidebarMenuItem>
  ));
};

export default ItemsSkeleton;
