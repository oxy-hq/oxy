import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { CommandSnippet } from "@/pages/admin/components/CommandSnippet";
import { INSTALL_METHODS, SKILLS_REPO_URL } from "./installMethods";

/**
 * The three ways to install the Oxy skills, as alternatives — tabs, not steps, because an
 * engineer needs exactly one of them.
 */
export const AgentSkillsInstall = () => (
  <div data-testid='admin-agent-skills-install' className='flex flex-col gap-2 p-3 text-xs'>
    <div className='flex flex-col gap-1'>
      <h3 className='font-semibold text-sm'>Oxy skills for coding agents</h3>
      <p className='text-muted-foreground leading-relaxed'>
        Teach an agent how Oxy works — custom apps and Oxy Functions, semantic models, agents,
        automations and the oxyc CLI — so it builds to the contract instead of guessing.
      </p>
    </div>

    <Tabs defaultValue={INSTALL_METHODS[0]?.id} className='gap-2'>
      <TabsList className='h-7'>
        {INSTALL_METHODS.map((m) => (
          <TabsTrigger key={m.id} value={m.id} className='text-xs'>
            {m.label}
          </TabsTrigger>
        ))}
      </TabsList>
      {INSTALL_METHODS.map((m) => (
        <TabsContent
          key={m.id}
          value={m.id}
          data-testid={`admin-agent-skills-method-${m.id}`}
          className='flex flex-col gap-1.5'
        >
          {m.commands.map((c) => (
            <CommandSnippet key={c} command={c} />
          ))}
          <p className='text-muted-foreground leading-relaxed'>{m.note}</p>
        </TabsContent>
      ))}
    </Tabs>

    <a
      href={SKILLS_REPO_URL}
      target='_blank'
      rel='noreferrer'
      data-testid='admin-agent-skills-repo-link'
      className='w-fit text-muted-foreground underline underline-offset-2 hover:text-foreground'
    >
      See every skill
    </a>
  </div>
);
