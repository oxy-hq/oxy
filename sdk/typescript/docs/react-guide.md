# React Context and Hooks Guide

Complete guide for using @oxy-hq/sdk with React Context and Hooks.

## Installation

```bash
npm install @oxy-hq/sdk @duckdb/duckdb-wasm
```

## Quick Start

### 1. Wrap Your App with OxyProvider

```tsx
import { OxyProvider } from '@oxy-hq/sdk';

export default function App() {
  return (
    <OxyProvider
      config={{
        baseUrl: 'https://api.oxy.tech',
        apiKey: 'your-api-key',
        projectId: 'your-project-uuid'
      }}
    >
      <YourApp />
    </OxyProvider>
  );
}
```

### 2. Use the SDK in Child Components

```tsx
import { useOxy } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

function Dashboard() {
  const { sdk, isLoading, error } = useOxy();
  const [data, setData] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadData() {
      await sdk.loadAppData('dashboard.app.yml');
      const result = await sdk.query('SELECT * FROM sales LIMIT 10');
      setData(result);
    }

    loadData();
  }, [sdk]);

  if (isLoading) return <div>Loading SDK...</div>;
  if (error) return <div>Error: {error.message}</div>;
  if (!data) return <div>Loading data...</div>;

  return (
    <table>
      <thead>
        <tr>
          {data.columns.map(col => <th key={col}>{col}</th>)}
        </tr>
      </thead>
      <tbody>
        {data.rows.map((row, i) => (
          <tr key={i}>
            {row.map((cell, j) => <td key={j}>{String(cell)}</td>)}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

---

## OxyProvider Props

```tsx
interface OxyProviderProps {
  config: Partial<OxyConfig>;
  useAsync?: boolean;
  onReady?: (sdk: OxySDK) => void;
  onError?: (error: Error) => void;
  children: React.ReactNode;
}
```

### Basic Configuration

```tsx
<OxyProvider
  config={{
    baseUrl: 'https://api.oxy.tech',
    apiKey: 'your-api-key',
    projectId: 'your-project-uuid',
    branch: 'main'  // optional
  }}
>
  <App />
</OxyProvider>
```

### With Environment Variables

```tsx
import { createConfig } from '@oxy-hq/sdk';

<OxyProvider config={createConfig()}>
  <App />
</OxyProvider>
```

Environment variables (for Vite, use `VITE_` prefix):
```env
VITE_OXY_URL=https://api.oxy.tech
VITE_OXY_API_KEY=your-api-key
VITE_OXY_PROJECT_ID=your-project-uuid
```

### With Callbacks

```tsx
<OxyProvider
  config={createConfig()}
  onReady={(sdk) => {
    console.log('SDK initialized successfully');
  }}
  onError={(error) => {
    console.error('SDK initialization failed:', error);
  }}
>
  <App />
</OxyProvider>
```

### Async Mode (PostMessage Auth)

For iframe scenarios where authentication comes from parent window:

```tsx
<OxyProvider
  useAsync={true}
  config={{
    parentOrigin: 'https://v0.dev',
    projectId: 'your-project-uuid',
    baseUrl: 'https://api.oxy.tech'
  }}
>
  <App />
</OxyProvider>
```

---

## useOxy Hook

Returns the SDK instance along with loading and error states.

```tsx
const { sdk, isLoading, error } = useOxy();
```

### Return Values

- `sdk: OxySDK | null` - The SDK instance (null while loading)
- `isLoading: boolean` - True while SDK is initializing
- `error: Error | null` - Error if initialization failed

### Example: Loading App Data

```tsx
function AppList() {
  const { sdk, isLoading, error } = useOxy();
  const [apps, setApps] = useState([]);

  useEffect(() => {
    if (!sdk) return;

    sdk.getClient().listApps()
      .then(setApps)
      .catch(console.error);
  }, [sdk]);

  if (isLoading) return <div>Initializing SDK...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <ul>
      {apps.map(app => (
        <li key={app.path}>{app.name}</li>
      ))}
    </ul>
  );
}
```

### Example: Querying Data

```tsx
function SalesReport() {
  const { sdk, isLoading, error } = useOxy();
  const [sales, setSales] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadSales() {
      // Load app data first
      await sdk.loadAppData('sales.app.yml');

      // Query the data
      const result = await sdk.query(`
        SELECT
          product,
          SUM(amount) as revenue,
          COUNT(*) as sales_count
        FROM sales
        GROUP BY product
        ORDER BY revenue DESC
        LIMIT 10
      `);

      setSales(result);
    }

    loadSales().catch(console.error);
  }, [sdk]);

  if (isLoading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;
  if (!sales) return <div>Loading sales data...</div>;

  return (
    <div>
      <h2>Top Products</h2>
      <table>
        <thead>
          <tr>
            {sales.columns.map(col => <th key={col}>{col}</th>)}
          </tr>
        </thead>
        <tbody>
          {sales.rows.map((row, i) => (
            <tr key={i}>
              {row.map((cell, j) => <td key={j}>{String(cell)}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

---

## useOxySDK Hook

Convenience hook that returns SDK directly or throws if not ready.

```tsx
const sdk = useOxySDK();
```

**Use this when:**
- SDK should always be ready (wrapped in loading check at parent level)
- You want cleaner code without null checks

**Example:**

```tsx
function App() {
  return (
    <OxyProvider config={createConfig()}>
      <LoadingBoundary>
        <DataView />
      </LoadingBoundary>
    </OxyProvider>
  );
}

function LoadingBoundary({ children }) {
  const { isLoading, error } = useOxy();

  if (isLoading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return children;
}

function DataView() {
  // No need to check if sdk is null - guaranteed to be ready
  const sdk = useOxySDK();
  const [data, setData] = useState(null);

  useEffect(() => {
    sdk.loadAppData('data.app.yml')
      .then(() => sdk.query('SELECT * FROM table'))
      .then(setData);
  }, []);

  return <div>{/* render data */}</div>;
}
```

---

## Common Patterns

### Pattern 1: Multiple Components, One Data Load

Load data once in parent, share with children:

```tsx
function Dashboard() {
  const { sdk, isLoading } = useOxy();
  const [appData, setAppData] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    // Load once
    sdk.loadAppData('dashboard.app.yml')
      .then(setAppData)
      .catch(console.error);
  }, [sdk]);

  if (isLoading || !appData) return <div>Loading...</div>;

  // All children can now query the loaded data
  return (
    <div>
      <SalesChart />
      <CustomerList />
      <ProductTable />
    </div>
  );
}

function SalesChart() {
  const sdk = useOxySDK();
  const [chartData, setChartData] = useState(null);

  useEffect(() => {
    // Data is already loaded, just query
    sdk.query('SELECT * FROM sales')
      .then(setChartData);
  }, []);

  return <div>{/* render chart */}</div>;
}
```

### Pattern 2: Custom Hook for Data Loading

```tsx
function useOxyQuery(sql: string) {
  const { sdk } = useOxy();
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    setLoading(true);
    sdk.query(sql)
      .then(setData)
      .catch(setError)
      .finally(() => setLoading(false));
  }, [sdk, sql]);

  return { data, loading, error };
}

// Usage
function TopProducts() {
  const { data, loading, error } = useOxyQuery(`
    SELECT product, SUM(amount) as revenue
    FROM sales
    GROUP BY product
    ORDER BY revenue DESC
    LIMIT 5
  `);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return <ProductList data={data} />;
}
```

### Pattern 3: Custom Hook for App Data

```tsx
function useOxyApp(appPath: string) {
  const { sdk } = useOxy();
  const [appData, setAppData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    setLoading(true);
    sdk.loadAppData(appPath)
      .then(setAppData)
      .catch(setError)
      .finally(() => setLoading(false));
  }, [sdk, appPath]);

  return { appData, loading, error, sdk };
}

// Usage
function Dashboard() {
  const { appData, loading, error, sdk } = useOxyApp('dashboard.app.yml');
  const [metrics, setMetrics] = useState(null);

  useEffect(() => {
    if (!appData || !sdk) return;

    // Now query the loaded data
    sdk.query('SELECT COUNT(*) as total FROM sales')
      .then(result => setMetrics(result.rows[0]))
      .catch(console.error);
  }, [appData, sdk]);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <div>
      <h2>Available Tables: {Object.keys(appData).join(', ')}</h2>
      {metrics && <p>Total Sales: {metrics[0]}</p>}
    </div>
  );
}
```

### Pattern 4: Context for Loaded Data

Share loaded data across components without prop drilling:

```tsx
import { createContext, useContext, useEffect, useState } from 'react';
import { useOxy } from '@oxy-hq/sdk';
import type { DataContainer } from '@oxy-hq/sdk';

const AppDataContext = createContext<DataContainer | null>(null);

export function AppDataProvider({ children }: { children: React.ReactNode }) {
  const { sdk, isLoading } = useOxy();
  const [appData, setAppData] = useState<DataContainer | null>(null);

  useEffect(() => {
    if (!sdk) return;

    sdk.loadAppData('dashboard.app.yml')
      .then(setAppData)
      .catch(console.error);
  }, [sdk]);

  if (isLoading || !appData) {
    return <div>Loading app data...</div>;
  }

  return (
    <AppDataContext.Provider value={appData}>
      {children}
    </AppDataContext.Provider>
  );
}

export function useAppData() {
  const context = useContext(AppDataContext);
  if (!context) {
    throw new Error('useAppData must be used within AppDataProvider');
  }
  return context;
}

// Usage
function App() {
  return (
    <OxyProvider config={createConfig()}>
      <AppDataProvider>
        <Dashboard />
      </AppDataProvider>
    </OxyProvider>
  );
}

function Dashboard() {
  const appData = useAppData();
  const sdk = useOxySDK();
  const [salesData, setSalesData] = useState(null);

  useEffect(() => {
    // Data is loaded, query it
    sdk.query('SELECT * FROM sales LIMIT 10')
      .then(setSalesData);
  }, []);

  return (
    <div>
      <h2>Tables: {Object.keys(appData).join(', ')}</h2>
      {salesData && <DataTable data={salesData} />}
    </div>
  );
}
```

### Pattern 5: Loading Multiple Apps

```tsx
function MultiAppDashboard() {
  const { sdk, isLoading } = useOxy();
  const [salesData, setSalesData] = useState(null);
  const [customerData, setCustomerData] = useState(null);
  const [inventoryData, setInventoryData] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadAll() {
      // Load multiple apps
      const [sales, customers, inventory] = await Promise.all([
        sdk.loadAppData('sales.app.yml'),
        sdk.loadAppData('customers.app.yml'),
        sdk.loadAppData('inventory.app.yml')
      ]);

      setSalesData(sales);
      setCustomerData(customers);
      setInventoryData(inventory);
    }

    loadAll().catch(console.error);
  }, [sdk]);

  if (isLoading) return <div>Loading...</div>;

  return (
    <div className="grid grid-cols-3 gap-4">
      <div>
        <h2>Sales</h2>
        {salesData && <div>{Object.keys(salesData).length} tables</div>}
      </div>
      <div>
        <h2>Customers</h2>
        {customerData && <div>{Object.keys(customerData).length} tables</div>}
      </div>
      <div>
        <h2>Inventory</h2>
        {inventoryData && <div>{Object.keys(inventoryData).length} tables</div>}
      </div>
    </div>
  );
}
```

---

## PostMessage Authentication (Iframe)

For embedding in iframes (like v0.dev), use async mode:

```tsx
export default function EmbeddedApp() {
  return (
    <OxyProvider
      useAsync={true}
      config={{
        parentOrigin: 'https://v0.dev',
        projectId: 'your-project-uuid',
        baseUrl: 'https://api.oxy.tech'
      }}
      onReady={(sdk) => console.log('Authenticated via postMessage')}
      onError={(err) => console.error('Auth failed:', err)}
    >
      <Dashboard />
    </OxyProvider>
  );
}

function Dashboard() {
  const { sdk, isLoading, error } = useOxy();

  if (isLoading) {
    return <div>Waiting for authentication...</div>;
  }

  if (error) {
    return <div>Authentication failed: {error.message}</div>;
  }

  // SDK is authenticated and ready
  return <YourContent />;
}
```

**Parent Window Setup:**

```typescript
window.addEventListener('message', (event) => {
  if (event.data.type !== 'OXY_AUTH_REQUEST') return;

  // Validate origin
  if (event.origin !== 'https://your-iframe-origin.com') return;

  // Send credentials
  event.source.postMessage({
    type: 'OXY_AUTH_RESPONSE',
    version: '1.0',
    requestId: event.data.requestId,
    apiKey: 'user-api-key',
    projectId: 'project-uuid',
    baseUrl: 'https://api.oxy.tech'
  }, event.origin);
});
```

---

## Remounting Provider

If you need to reinitialize the SDK (e.g., for auth changes), use a `key` prop:

```tsx
function App() {
  const [configKey, setConfigKey] = useState(0);
  const [config, setConfig] = useState(null);

  const handleLogin = (newConfig) => {
    setConfig(newConfig);
    setConfigKey(prev => prev + 1); // Force remount
  };

  return (
    <OxyProvider key={configKey} config={config}>
      <Dashboard />
    </OxyProvider>
  );
}
```

---

## Error Handling

### Handle Errors at Provider Level

```tsx
function App() {
  const [sdkError, setSdkError] = useState(null);

  return (
    <OxyProvider
      config={createConfig()}
      onError={(error) => {
        console.error('SDK Error:', error);
        setSdkError(error);
      }}
    >
      {sdkError ? (
        <ErrorFallback error={sdkError} />
      ) : (
        <Dashboard />
      )}
    </OxyProvider>
  );
}
```

### Handle Errors in Components

```tsx
function DataView() {
  const { sdk, error: sdkError } = useOxy();
  const [data, setData] = useState(null);
  const [queryError, setQueryError] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    sdk.query('SELECT * FROM sales')
      .then(setData)
      .catch(setQueryError);
  }, [sdk]);

  if (sdkError) {
    return <div>SDK Error: {sdkError.message}</div>;
  }

  if (queryError) {
    return <div>Query Error: {queryError.message}</div>;
  }

  return data ? <DataTable data={data} /> : <div>Loading...</div>;
}
```

---

## TypeScript Support

The SDK is fully typed. Import types as needed:

```tsx
import type {
  OxySDK,
  OxyConfig,
  QueryResult,
  DataContainer
} from '@oxy-hq/sdk';

function DataView() {
  const { sdk } = useOxy();
  const [result, setResult] = useState<QueryResult | null>(null);

  useEffect(() => {
    if (!sdk) return;

    sdk.query('SELECT * FROM sales')
      .then((data: QueryResult) => setResult(data));
  }, [sdk]);

  return result ? (
    <div>
      <p>Columns: {result.columns.join(', ')}</p>
      <p>Rows: {result.rowCount}</p>
    </div>
  ) : null;
}
```

---

## Best Practices

1. **Always use OxyProvider** - Let it handle initialization and cleanup
2. **Check loading state** - Always check `isLoading` before using SDK
3. **Handle errors** - Always handle `error` from `useOxy()`
4. **Load data once** - Load in parent, query in children
5. **Cleanup is automatic** - OxyProvider cleans up when unmounted
6. **Use TypeScript** - Get full type safety and autocomplete
7. **Avoid re-initialization** - Don't create new SDK instances, use the context
8. **PostMessage security** - Always specify exact `parentOrigin`, never `'*'`
9. **Environment variables** - Use `VITE_` prefix for Vite projects
10. **Error boundaries** - Wrap OxyProvider in error boundary for production

---

## Common Issues

### Issue: "useOxy must be used within an OxyProvider"

**Cause**: Component using `useOxy()` is not wrapped in `<OxyProvider>`.

**Solution**: Wrap your app with OxyProvider:

```tsx
// ✅ Correct
<OxyProvider config={config}>
  <ComponentUsingUseOxy />
</OxyProvider>

// ❌ Wrong
<ComponentUsingUseOxy />
```

### Issue: "sdk is null"

**Cause**: SDK not finished initializing.

**Solution**: Check loading state:

```tsx
const { sdk, isLoading } = useOxy();

if (isLoading) return <div>Loading...</div>;
if (!sdk) return <div>SDK not available</div>;

// Now safe to use sdk
```

### Issue: "Table not found"

**Cause**: Querying before data is loaded.

**Solution**: Always load data before querying:

```tsx
useEffect(() => {
  if (!sdk) return;

  async function load() {
    await sdk.loadAppData('app.yml'); // Load first
    const result = await sdk.query('SELECT * FROM table'); // Then query
    setData(result);
  }

  load();
}, [sdk]);
```

### Issue: PostMessage timeout

**Cause**: Parent window not responding.

**Solution**: Verify `parentOrigin` is correct and parent has listener:

```tsx
<OxyProvider
  useAsync={true}
  config={{
    parentOrigin: 'https://exact-parent-origin.com', // Must match exactly
    timeout: 10000 // Increase timeout if needed
  }}
>
```

---

## Quick Reference

### Provider Setup

```tsx
<OxyProvider config={config} useAsync={boolean} onReady={fn} onError={fn}>
  <App />
</OxyProvider>
```

### Hooks

```tsx
// Returns SDK with loading/error states
const { sdk, isLoading, error } = useOxy();

// Returns SDK or throws
const sdk = useOxySDK();
```

### SDK Methods

```tsx
await sdk.loadFile(filePath, tableName)
await sdk.loadAppData(appPath)
await sdk.query(sql)
await sdk.getAll(tableName, limit)
await sdk.getSchema(tableName)
await sdk.count(tableName)
await sdk.close()
```

### Config

```tsx
{
  baseUrl: string,
  apiKey?: string,
  projectId: string,
  branch?: string,
  parentOrigin?: string,  // For postMessage auth
  disableAutoAuth?: boolean,
  timeout?: number
}
```
