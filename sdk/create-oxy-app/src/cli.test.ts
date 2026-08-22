import { describe, expect, test } from "vitest";
import { parseArgs, slugify, templateDestName } from "./cli.js";

describe("parseArgs", () => {
  test("captures the first positional as name, default template is vite", () => {
    expect(parseArgs(["my-app"])).toMatchObject({ name: "my-app", template: "vite" });
  });

  test("--template <id>", () => {
    expect(parseArgs(["my-app", "--template", "dashboard"])).toMatchObject({
      name: "my-app",
      template: "dashboard"
    });
  });

  test("--template=<id>", () => {
    expect(parseArgs(["my-app", "--template=single-store"])).toMatchObject({
      template: "single-store"
    });
  });

  test("--template functions (the opt-in server-side template)", () => {
    expect(parseArgs(["my-app", "--template", "functions"])).toMatchObject({
      name: "my-app",
      template: "functions"
    });
  });

  test("-t <id> short form", () => {
    expect(parseArgs(["x", "-t", "dashboard"]).template).toBe("dashboard");
  });

  test("--help", () => {
    expect(parseArgs(["--help"]).help).toBe(true);
    expect(parseArgs(["-h"]).help).toBe(true);
  });

  test("ignores subsequent positionals", () => {
    expect(parseArgs(["first", "second", "third"]).name).toBe("first");
  });
});

describe("slugify", () => {
  test("lowercases", () => {
    expect(slugify("StorePulse")).toBe("storepulse");
  });

  test("replaces spaces with dashes", () => {
    expect(slugify("Store Pulse App")).toBe("store-pulse-app");
  });

  test("collapses runs of dashes", () => {
    expect(slugify("a -- b")).toBe("a-b");
  });

  test("strips leading and trailing dashes", () => {
    expect(slugify("--leading-trailing--")).toBe("leading-trailing");
  });

  test("strips non-alphanumeric characters", () => {
    expect(slugify("hello!world@2024")).toBe("hello-world-2024");
  });
});

describe("templateDestName", () => {
  test("_gitignore becomes .gitignore (npm strips real dotfiles)", () => {
    expect(templateDestName("_gitignore")).toBe(".gitignore");
  });

  // Both spellings must be stripped: the server-side scaffold
  // (crates/app/src/custom_app_template/mod.rs) filters `.yml.example` AND
  // `.yaml.example` so those files never land in the customer-apps monorepo,
  // so a standalone CLI scaffold has to rename both back or it loses them.
  test("strips .yml.example (shared deploy workflow)", () => {
    expect(templateDestName("deploy.yml.example")).toBe("deploy.yml");
  });

  test("strips .yaml.example (the functions template's pnpm workspace root)", () => {
    expect(templateDestName("pnpm-workspace.yaml.example")).toBe("pnpm-workspace.yaml");
  });

  test("leaves ordinary filenames alone", () => {
    expect(templateDestName("package.json")).toBe("package.json");
    expect(templateDestName("vite.config.ts")).toBe("vite.config.ts");
  });
});
