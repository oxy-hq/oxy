/**
 * How to give a coding agent the Oxy skills — one list, read by every surface that offers it.
 *
 * The skills live in `sdk/cli/skills/` and are published to `oxy-hq/skills`; these are the
 * three ways that repo installs. Keep them in step with its README (rendered by
 * `sdk/cli/scripts/skills-repo.mjs`).
 */

export const SKILLS_REPO_URL = "https://github.com/oxy-hq/skills";

/** The one command that works for every agent, so it is the one a reminder leads with. */
export const QUICK_INSTALL = "npx skills add oxy-hq/skills";

export type InstallMethod = {
  id: "any-agent" | "claude-code" | "oxyc";
  label: string;
  commands: string[];
  note: string;
};

export const INSTALL_METHODS: InstallMethod[] = [
  {
    id: "any-agent",
    label: "Any agent",
    commands: [QUICK_INSTALL],
    note: "For Claude Code, Cursor, Codex, Copilot and any agent that reads Agent Skills. Add -g to install for every project; npx skills update keeps them current."
  },
  {
    id: "claude-code",
    label: "Claude Code",
    commands: ["/plugin marketplace add oxy-hq/skills", "/plugin install oxy@oxy-hq"],
    note: "Run inside Claude Code. Turn on auto-update for the oxy-hq marketplace under /plugin to stay current."
  },
  {
    id: "oxyc",
    label: "oxyc",
    commands: ["npm install -g @oxy-hq/cli", "oxyc skills install"],
    note: "Links the skills that ship with oxyc into ~/.claude/skills. They update when oxyc does."
  }
];
