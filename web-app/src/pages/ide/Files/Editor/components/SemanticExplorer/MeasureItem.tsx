import { SidebarMenuSubButton, SidebarMenuSubItem } from "@/components/ui/shadcn/sidebar";

interface MeasureItemProps {
  name: string;
  isSelected: boolean;
  onToggle: () => void;
}

const MeasureItem = ({ name, isSelected, onToggle }: MeasureItemProps) => (
  <SidebarMenuSubItem>
    <SidebarMenuSubButton
      onClick={onToggle}
      isActive={isSelected}
      data-testid={`semantic-field-measure-${name.replace(/[^A-Za-z0-9_]/g, "_")}`}
    >
      <span>{name}</span>
    </SidebarMenuSubButton>
  </SidebarMenuSubItem>
);

export default MeasureItem;
