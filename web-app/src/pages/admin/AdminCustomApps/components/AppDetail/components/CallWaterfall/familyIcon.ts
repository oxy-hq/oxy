import {
  ArrowLeftRight,
  Bell,
  CircleDot,
  Database,
  Globe,
  HardDrive,
  KeyRound,
  type LucideIcon,
  Mail,
  Rows3,
  Server,
  Sigma,
  Table2,
  Users,
  Warehouse,
  Workflow
} from "lucide-react";

/** The glyph beside each call's family word. Shape, not colour, tells the
 *  families apart: a bar's colour is its status alone. */
const FAMILY_ICON: Record<string, LucideIcon> = {
  query: Database,
  query_stream: Rows3,
  fetch: Globe,
  semantic: Sigma,
  airway: Workflow,
  warehouse: Warehouse,
  airhouse: Warehouse,
  oltp: Table2,
  tx: ArrowLeftRight,
  secrets: KeyRound,
  email: Mail,
  notify: Bell,
  storage: HardDrive,
  org: Users,
  platform: Server
};

export const familyIcon = (family: string): LucideIcon => FAMILY_ICON[family] ?? CircleDot;
