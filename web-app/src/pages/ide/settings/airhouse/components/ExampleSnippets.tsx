import { Copy } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/shadcn/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type { AirhouseConnectionInfo } from "@/services/api";

const PASSWORD_PLACEHOLDER = "<password>";

const renderPsql = (c: AirhouseConnectionInfo, password: string) =>
  `psql "host=${c.host} port=${c.port} dbname=${c.dbname} user=${c.username} password=${password}"`;

const renderJdbc = (c: AirhouseConnectionInfo, password: string) =>
  `jdbc:postgresql://${c.host}:${c.port}/${c.dbname}?user=${c.username}&password=${password}`;

const renderPython = (c: AirhouseConnectionInfo, password: string) =>
  [
    "import psycopg2",
    "conn = psycopg2.connect(",
    `    host="${c.host}", port=${c.port},`,
    `    dbname="${c.dbname}", user="${c.username}", password="${password}"`,
    ")",
    "conn.set_session(autocommit=True)"
  ].join("\n");

interface ExampleSnippetsProps {
  connection: AirhouseConnectionInfo;
  /** When set, snippets show the actual password; otherwise a placeholder. */
  password: string | null;
}

export const ExampleSnippets: React.FC<ExampleSnippetsProps> = ({ connection, password }) => {
  const [tab, setTab] = useState("psql");
  const pw = password ?? PASSWORD_PLACEHOLDER;

  const snippets: Record<string, string> = {
    psql: renderPsql(connection, pw),
    jdbc: renderJdbc(connection, pw),
    python: renderPython(connection, pw)
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(snippets[tab]);
      toast.success("Copied snippet");
    } catch {
      toast.error("Failed to copy to clipboard");
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Example snippets</CardTitle>
        {!password ? (
          <p className='text-muted-foreground text-sm'>
            Reveal your password above to embed it directly in these snippets.
          </p>
        ) : (
          <p className='text-muted-foreground text-sm'>
            These snippets contain your live password. Copy them carefully.
          </p>
        )}
      </CardHeader>
      <CardContent>
        <Tabs value={tab} onValueChange={setTab}>
          <div className='flex items-center justify-between gap-2'>
            <TabsList>
              <TabsTrigger value='psql'>psql</TabsTrigger>
              <TabsTrigger value='jdbc'>JDBC</TabsTrigger>
              <TabsTrigger value='python'>Python</TabsTrigger>
            </TabsList>
            <Button variant='outline' size='sm' onClick={handleCopy}>
              <Copy className='h-4 w-4' />
              Copy
            </Button>
          </div>
          {Object.entries(snippets).map(([key, value]) => (
            <TabsContent key={key} value={key} className='mt-3'>
              <pre className='overflow-x-auto rounded-md border bg-muted p-3 font-mono text-xs'>
                {value}
              </pre>
            </TabsContent>
          ))}
        </Tabs>
      </CardContent>
    </Card>
  );
};
