import {
  Database,
  Shield,
  Users,
  Key,
  FileText,
  X,
  Github,
} from "lucide-react";
import { useState } from "react";
import { Dialog, DialogContent } from "../ui/shadcn/dialog";
import GithubSettings from "./github";
import SecretManagement from "./secrets";
import DatabaseManagement from "./databases";
import UserManagement from "./users";
import ApiKeyManagement from "./api-keys";
import LogsManagement from "./activity-logs";
import { Button } from "../ui/shadcn/button";
import useSettingsPage from "@/stores/useSettingsPage";

interface SettingsSection {
  id: string;
  title: string;
  description: string;
  icon: React.ReactNode;
  show?: boolean;
  page: React.ReactNode;
}

export function SettingsModal() {
  const { isOpen, setIsOpen } = useSettingsPage();
  const [activeSection, setActiveSection] = useState<string>("github-settings");

  const settingsSections: SettingsSection[] = [
    {
      id: "github-settings",
      title: "Github Settings",
      description: "Configure GitHub integration",
      icon: <Github className="w-4 h-4" />,
      show: true,
      page: <GithubSettings />,
    },
    {
      id: "secrets",
      title: "Secret Management",
      description: "Manage sensitive data",
      icon: <Shield className="w-4 h-4" />,
      show: true,
      page: <SecretManagement />,
    },
    {
      id: "databases",
      title: "Databases",
      description: "Database connections",
      icon: <Database className="w-4 h-4" />,
      show: true,
      page: <DatabaseManagement />,
    },
    {
      id: "users",
      title: "Users",
      description: "User management",
      icon: <Users className="w-4 h-4" />,
      show: true,
      page: <UserManagement />,
    },
    {
      id: "api-keys",
      title: "API Keys",
      description: "External access keys",
      icon: <Key className="w-4 h-4" />,
      show: true,
      page: <ApiKeyManagement />,
    },
    {
      id: "logs",
      title: "Activity Logs",
      description: "System audit logs",
      icon: <FileText className="w-4 h-4" />,
      show: true,
      page: <LogsManagement />,
    },
  ];

  const visibleSections = settingsSections.filter((section) => section.show);

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogContent
        showCloseButton={false}
        className="!max-w-6xl w-full h-[85vh] p-0 overflow-hidden"
      >
        <div className="flex h-full overflow-hidden">
          <div className="py-4 px-2 w-64 flex border-r flex-col gap-2 overflow-auto customScrollbar bg-sidebar">
            <div>
              <Button
                variant="ghost"
                onClick={() => setIsOpen(false)}
                className="w-auto"
              >
                <X className="w-4 h-4" />
              </Button>
            </div>

            {visibleSections.map((section) => (
              <Button
                key={section.id}
                variant={activeSection !== section.id ? "ghost" : "default"}
                className="justify-start"
                onClick={() => setActiveSection(section.id)}
              >
                {section.icon}
                {section.title}
              </Button>
            ))}
          </div>

          <div className="flex-1 overflow-auto customScrollbar scrollbar-gutter-auto">
            {
              visibleSections.find((section) => section.id === activeSection)
                ?.page
            }
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
