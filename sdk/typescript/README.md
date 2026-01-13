# Oxy TypeScript SDK

Official TypeScript/JavaScript SDK for interacting with the Oxy data platform.

## Features

- 🚀 **Simple API** - Easy-to-use client for fetching app data
- 📊 **Parquet Support** - Read and query Parquet files using DuckDB-WASM
- 🔒 **Type-Safe** - Full TypeScript support with comprehensive type definitions
- 🌐 **Universal** - Works in both Node.js and browser environments
- ⚡ **Fast** - Optimized for performance with efficient data handling

## Installation

```bash
npm install @oxy/sdk
# or
yarn add @oxy/sdk
# or
pnpm add @oxy/sdk
```

For Parquet file support, also install DuckDB-WASM:

```bash
npm install @duckdb/duckdb-wasm
```

## Quick Start

### Basic Usage

```typescript
import { OxySDK } from "@oxy/sdk";

// Create SDK instance
const sdk = new OxySDK({
  apiKey: "your-api-key",
  projectId: "your-project-id",
  baseUrl: "https://api.oxy.tech",
});

// Load parquet files and query them
await sdk.loadFile("data/sales.parquet", "sales");
await sdk.loadFile("data/customers.parquet", "customers");

// Query with SQL - supports joins across multiple tables
const result = await sdk.query(`
  SELECT s.product, s.amount, c.name as customer_name
  FROM sales s
  JOIN customers c ON s.customer_id = c.id
  WHERE s.amount > 1000
  ORDER BY s.amount DESC
`);

console.log(result.rows);
await sdk.close();
```

### Load App Data Automatically

```typescript
import { OxySDK, createConfig } from "@oxy/sdk";

const sdk = new OxySDK(createConfig());

// Loads all data from the app and registers tables
await sdk.loadAppData("dashboard.app.yml");

// Query the loaded tables
const result = await sdk.query("SELECT * FROM my_table LIMIT 10");
```

### Iframe Usage (PostMessage Authentication)

For embedding in iframes (e.g., v0.dev, sandboxed environments):

```typescript
import { OxySDK } from "@oxy/sdk";

// SDK automatically requests API key from parent window
const sdk = await OxySDK.create({
  parentOrigin: "https://app.example.com",
  projectId: "your-project-uuid",
});

await sdk.loadAppData("dashboard.app.yml");
const result = await sdk.query("SELECT * FROM my_table LIMIT 10");
```

**Parent window setup:**

```typescript
window.addEventListener("message", (event) => {
  if (event.data.type !== "OXY_AUTH_REQUEST") return;
  if (event.origin !== "https://your-iframe-app.com") return;

  event.source.postMessage(
    {
      type: "OXY_AUTH_RESPONSE",
      version: "1.0",
      requestId: event.data.requestId,
      apiKey: getUserApiKey(),
      projectId: "your-project-uuid",
      baseUrl: "https://api.oxy.tech",
    },
    event.origin,
  );
});
```

### Environment Variables

```bash
export OXY_URL="https://api.oxy.tech"
export OXY_API_KEY="your-api-key"
export OXY_PROJECT_ID="your-project-uuid"
export OXY_BRANCH="main"  # optional
```

Use `createConfig()` to load from environment:

```typescript
import { OxySDK, createConfig } from "@oxy/sdk";

const sdk = new OxySDK(createConfig());
```

## React Integration

### Using React Context (Recommended)

The SDK provides `OxyProvider` and `useOxy` hooks for easy integration:

```tsx
import { OxyProvider, useOxy, createConfig } from "@oxy/sdk";
import { useEffect, useState } from "react";

// Wrap your app with OxyProvider
function App() {
  return (
    <OxyProvider config={createConfig()}>
      <Dashboard />
    </OxyProvider>
  );
}

// Access SDK in child components
function Dashboard() {
  const { sdk, isLoading, error } = useOxy();
  const [data, setData] = useState(null);

  useEffect(() => {
    if (sdk) {
      sdk
        .loadAppData("dashboard.app.yml")
        .then(() => sdk.query("SELECT * FROM my_table LIMIT 100"))
        .then(setData);
    }
  }, [sdk]);

  if (isLoading) return <div>Initializing SDK...</div>;
  if (error) return <div>Error: {error.message}</div>;
  if (!data) return <div>Loading data...</div>;

  return (
    <div>
      <h1>Dashboard</h1>
      <table>
        <thead>
          <tr>
            {data.columns.map((col) => (
              <th key={col}>{col}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.rows.map((row, i) => (
            <tr key={i}>
              {row.map((cell, j) => (
                <td key={j}>{String(cell)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

### Iframe with PostMessage Auth

```tsx
import { OxyProvider, useOxySDK } from "@oxy/sdk";

function App() {
  return (
    <OxyProvider useAsync config={{ parentOrigin: "https://app.example.com" }}>
      <Dashboard />
    </OxyProvider>
  );
}

function Dashboard() {
  const sdk = useOxySDK(); // Throws if not ready
  const [data, setData] = useState(null);

  useEffect(() => {
    sdk
      .loadFile("data/sales.parquet", "sales")
      .then(() => sdk.query("SELECT * FROM sales LIMIT 100"))
      .then(setData);
  }, [sdk]);

  return <div>{/* render data */}</div>;
}
```

### Without Context (Alternative)

```typescript
import { OxySDK, createConfig } from '@oxy/sdk';
import { useEffect, useState } from 'react';

const sdk = new OxySDK(createConfig());

function Dashboard() {
  const [data, setData] = useState(null);

  useEffect(() => {
    sdk.loadAppData('dashboard.app.yml')
      .then(() => sdk.query('SELECT * FROM my_table LIMIT 100'))
      .then(setData);
  }, []);

  return <div>{/* render data */}</div>;
}
```

## Use with v0 and Sandbox Services

**For AI Assistants (v0.dev, Cursor, etc.):** See [.v0/rules.md](.v0/rules.md) and [.cursorrules](.cursorrules) for integration guidelines.

```typescript
import { OxySDK, createConfig } from "@oxy/sdk";

const sdk = new OxySDK(createConfig());

export async function getDashboardData() {
  await sdk.loadAppData("dashboard.app.yml");

  return await sdk.query(`
    SELECT s.*, c.name as customer_name
    FROM sales s
    LEFT JOIN customers c ON s.customer_id = c.id
    ORDER BY s.date DESC
    LIMIT 100
  `);
}

export function getChartUrl() {
  return sdk.getClient().getFileUrl("charts/sales-overview.png");
}
```

## API Reference

### OxySDK (Unified Interface)

The `OxySDK` class combines `OxyClient` and `ParquetReader` into a single, easy-to-use interface.

#### `constructor(config: OxyConfig)`

Creates a new SDK instance.

#### `static async create(config?: Partial<OxyConfig>): Promise<OxySDK>`

Creates an SDK instance with async configuration (supports postMessage auth).

```typescript
const sdk = await OxySDK.create({
  parentOrigin: "https://app.example.com",
  projectId: "your-project-id",
});
```

#### `async loadFile(filePath: string, tableName: string): Promise<void>`

Loads a Parquet file from Oxy and registers it for SQL queries.

```typescript
await sdk.loadFile("data/sales.parquet", "sales");
```

#### `async loadFiles(files: Array<{filePath: string, tableName: string}>): Promise<void>`

Loads multiple Parquet files at once.

```typescript
await sdk.loadFiles([
  { filePath: "data/sales.parquet", tableName: "sales" },
  { filePath: "data/customers.parquet", tableName: "customers" },
]);
```

#### `async loadAppData(appPath: string): Promise<DataContainer | null>`

Loads all data from an app's data container. Uses container keys as table names.

```typescript
const data = await sdk.loadAppData("dashboard.app.yml");
// Now query the tables using their container keys
const result = await sdk.query("SELECT * FROM my_table");
```

#### `async query(sql: string): Promise<QueryResult>`

Executes a SQL query against loaded data.

```typescript
const result = await sdk.query("SELECT * FROM sales WHERE amount > 1000");
```

#### `async getAll(tableName: string, limit?: number): Promise<QueryResult>`

Gets all data from a loaded table.

```typescript
const data = await sdk.getAll("sales", 100);
```

#### `async getSchema(tableName: string): Promise<QueryResult>`

Gets schema information for a loaded table.

#### `async count(tableName: string): Promise<number>`

Gets row count for a loaded table.

#### `getClient(): OxyClient`

Returns the underlying `OxyClient` for advanced operations.

```typescript
const apps = await sdk.getClient().listApps();
```

#### `getReader(): ParquetReader`

Returns the underlying `ParquetReader` for advanced operations.

#### `async close(): Promise<void>`

Closes and cleans up all resources.

### React Hooks

#### `OxyProvider`

Provider component that initializes and provides OxySDK to child components.

**Props:**

- `config?: Partial<OxyConfig>` - SDK configuration
- `useAsync?: boolean` - If true, uses async initialization (supports postMessage auth)
- `onReady?: (sdk: OxySDK) => void` - Called when SDK is initialized
- `onError?: (error: Error) => void` - Called on initialization error

```tsx
<OxyProvider config={createConfig()}>
  <YourApp />
</OxyProvider>
```

#### `useOxy()`

Hook to access SDK, loading state, and errors.

```tsx
const { sdk, isLoading, error } = useOxy();
```

Returns:

- `sdk: OxySDK | null` - The SDK instance (null if not ready)
- `isLoading: boolean` - True while initializing
- `error: Error | null` - Initialization error if any

#### `useOxySDK()`

Hook that returns SDK directly or throws if not ready. Useful when you know SDK should be initialized.

```tsx
const sdk = useOxySDK(); // Throws if not ready
```

### Advanced: OxyClient

For advanced use cases, access the underlying client via `sdk.getClient()`:

```typescript
const client = sdk.getClient();
await client.listApps();
await client.getDisplays("my-app.app.yml");
const blob = await client.getFile("path/to/file.parquet");
```

### Advanced: ParquetReader

For advanced use cases, access the underlying reader via `sdk.getReader()`:

```typescript
const reader = sdk.getReader();
await reader.registerParquet(customBlob, "custom_table");
```

Or use standalone:

```typescript
import { ParquetReader } from "@oxy/sdk";

const reader = new ParquetReader();
await reader.registerParquet(blob1, "table1");
await reader.registerParquet(blob2, "table2");

const result = await reader.query("SELECT * FROM table1 JOIN table2 ON ...");
await reader.close();
```

## Environment Variables

- `OXY_URL` - Base URL of the Oxy API (required)
- `OXY_API_KEY` - API key for authentication (required)
- `OXY_PROJECT_ID` - Project UUID (required)
- `OXY_BRANCH` - Branch name (optional)

## Examples

See the [examples](./examples) directory for more detailed examples:

## Building and Publishing

```bash
# Install dependencies
npm install

# Build the SDK
npm run build

# Type check
npm run typecheck

# Lint
npm run lint

# Publish to npm (beta)
npm run publish:beta

# Publish to npm (latest)
npm run publish:latest
```

## License

MIT

## Support

For issues and questions, please visit [GitHub Issues](https://github.com/dataframehq/oxy-internal/issues).

### Local development with v0 or cloud service

- Disable the local network access check [flag](chrome ://flags/#local-network-access-check)
