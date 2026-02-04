import { Github, Key, Shield, Users, X } from "lucide-react";
import { useState } from "react";
import { useAuth } from "@/contexts/AuthContext";
import useSettingsPage from "@/stores/useSettingsPage";
import { Button } from "../ui/shadcn/button";
import { Dialog, DialogContent } from "../ui/shadcn/dialog";
import ApiKeyManagement from "./api-keys";
import GithubSettings from "./github";
import SecretManagement from "./secrets";
import UserManagement from "./users";

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
      icon: <Github className='h-4 w-4' />,
      show: authConfig.cloud,
      page: <GithubSettings />
    },
    {
      id: "secrets",
      title: "Secret Management",
      description: "Manage sensitive data",
      icon: <Shield className='h-4 w-4' />,
      show: authConfig.cloud,
      page: <SecretManagement />
    },
    {
      id: "users",
      title: "Users",
      description: "User management",
      icon: <Users className='h-4 w-4' />,
      show: authConfig.cloud,
      page: <UserManagement />
    },
    {
      id: "api-keys",
      title: "API Keys",
      description: "External access keys",
      icon: <Key className='h-4 w-4' />,
      show: authConfig.cloud,
      page: <ApiKeyManagement />
    }
  ];

  const visibleSections = settingsSections.filter((section) => section.show);

  return (
    <Dialog open={isOpen} onOpenChange={setIsOpen}>
      <DialogContent
        showCloseButton={false}
        className='!max-w-6xl h-[85vh] w-full overflow-hidden p-0'
      >
        <div className='flex h-full overflow-hidden'>
          <div className='customScrollbar flex w-64 flex-col gap-2 overflow-auto border-r bg-sidebar px-2 py-4'>
            <div>
              <Button variant='ghost' onClick={() => setIsOpen(false)} className='w-auto'>
                <X className='h-4 w-4' />
              </Button>
            </div>

            {visibleSections.map((section) => (
              <Button
                key={section.id}
                variant={activeSection !== section.id ? "ghost" : "default"}
                className='justify-start'
                onClick={() => setActiveSection(section.id)}
              >
                {section.icon}
                {section.title}
              </Button>
            ))}
          </div>

          <div className='customScrollbar scrollbar-gutter-auto flex-1 overflow-auto'>
            {visibleSections.find((section) => section.id === activeSection)?.page}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
