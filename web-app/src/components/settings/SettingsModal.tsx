import { Shield, Users, Key, X, Github } from "lucide-react";
import { useState } from "react";
import { Dialog, DialogContent } from "../ui/shadcn/dialog";
import GithubSettings from "./github";
import SecretManagement from "./secrets";
import UserManagement from "./users";
import ApiKeyManagement from "./api-keys";
import { Button } from "../ui/shadcn/button";
import useSettingsPage from "@/stores/useSettingsPage";
import { useAuth } from "@/contexts/AuthContext";

interface SettingsSection {
  id: string;
  title: string;
  description: string;
  icon: React.ReactNode;
  show?: boolean;
  page: React.ReactNode;
}

export function SettingsModal() {
  const { authConfig } = useAuth();
  const { isOpen, setIsOpen } = useSettingsPage();
  const [activeSection, setActiveSection] = useState<string>("github-settings");

  const settingsSections: SettingsSection[] = [
    {
      id: "github-settings",
      title: "Github Settings",
      description: "Configure GitHub integration",
      icon: <Github className="w-4 h-4" />,
      show: authConfig.cloud,
      page: <GithubSettings />,
    },
    {
      id: "secrets",
      title: "Secret Management",
      description: "Manage sensitive data",
      icon: <Shield className="w-4 h-4" />,
      show: authConfig.cloud,
      page: <SecretManagement />,
    },
    {
      id: "users",
      title: "Users",
      description: "User management",
      icon: <Users className="w-4 h-4" />,
      show: authConfig.cloud,
      page: <UserManagement />,
    },
    {
      id: "api-keys",
      title: "API Keys",
      description: "External access keys",
      icon: <Key className="w-4 h-4" />,
      show: authConfig.cloud,
      page: <ApiKeyManagement />,
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
