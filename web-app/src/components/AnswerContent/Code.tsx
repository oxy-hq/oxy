import { ComponentProps, memo } from "react";

import { python } from "@codemirror/lang-python";
import { sql } from "@codemirror/lang-sql";
import { LanguageSupport } from "@codemirror/language";
import CodeMirror from "@uiw/react-codemirror";
import { tags as t } from "@lezer/highlight";
import { createTheme } from "@uiw/codemirror-themes";

export type SupportedLanguages = "python" | "sql";

export const getLangs = (name?: SupportedLanguages, codeContent?: string) => {
  if (!name && codeContent) {
    // Simple heuristic to detect language based on code content
    if (codeContent.includes("def ") || codeContent.includes("import ")) {
      name = "python";
    } else if (
      codeContent.includes("SELECT ") ||
      codeContent.includes("FROM ")
    ) {
      name = "sql";
    }
  }

  if (!name) return python;

  const langs: Record<SupportedLanguages, () => LanguageSupport> = {
    python,
    sql,
  };
  return langs[name] || python;
};

const containerStyles = "p-4 rounded-lg border border-border bg-zinc-900";

const theme = createTheme({
  theme: "light",
  settings: {
    background: "var(--colors-zinc-900)",
    foreground: "var(--color-code-primary)",
    fontFamily: "var(--font-family-geist-mono)",
  },
  styles: [
    // { tag: t.comment, color: "var(--color-code-primary)" },
    { tag: t.lineComment, color: "var(--color-code-primary)" },
    { tag: t.literal, color: "var(--color-code-primary)" },
    { tag: t.definition(t.typeName), color: "var(--color-code-primary)" },
    { tag: t.moduleKeyword, color: "var(--color-code-primary)" },
    { tag: t.keyword, color: "var(--color-code-primary)" },
    { tag: t.number, color: "var(--color-code-primary)" },
    { tag: t.function(t.propertyName), color: "var(--color-code-primary)" },
  ],
});

type Props = {
  value: string;
  lang?: SupportedLanguages;
};

// memoize to prevent rerendering
const CodePreview = memo(function Code(props: Props) {
  const { value, lang } = props;
  const language = getLangs(lang, value);
  return (
    <div className={containerStyles}>
      <CodeMirror
        value={value}
        extensions={[theme, language()]}
        basicSetup={{
          lineNumbers: false,
          foldGutter: false,
          highlightActiveLine: false,
        }}
        theme={theme}
        readOnly
        editable={false}
      />
    </div>
  );
});
CodePreview.displayName = "CodePreview";

export default function CodeContainer(props: ComponentProps<"code">) {
  const { children, className } = props;
  const match = /language-(\w+)/.exec(className || "");
  const lang = match?.[1];

  if (typeof children === "string") {
    const value = children.trim();
    return <CodePreview value={value} lang={lang as SupportedLanguages} />;
  }
  return null;
}
