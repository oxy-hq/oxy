# Comprehensive Examples for @oxy-hq/sdk

Complete guide for using the Oxy SDK in v0.dev and other React environments.

## Table of Contents

1. [Installation](#installation)
2. [Basic Usage](#basic-usage)
3. [React Integration](#react-integration)
4. [Iframe & PostMessage Auth](#iframe--postmessage-auth)
5. [Complete Working Examples](#complete-working-examples)
6. [Common Patterns](#common-patterns)
7. [Troubleshooting](#troubleshooting)

---

## Installation

```bash
npm install @oxy-hq/sdk @duckdb/duckdb-wasm
```

For Vite projects (like v0.dev), environment variables use the `VITE_` prefix:

```env
VITE_OXY_URL=https://api.oxy.tech
VITE_OXY_API_KEY=your-api-key
VITE_OXY_PROJECT_ID=your-project-uuid
VITE_OXY_BRANCH=main
```

---

## Basic Usage

### 1. Simple File Loading and Query

```typescript
import { OxySDK, createConfig } from '@oxy-hq/sdk';

async function example() {
  // Create SDK instance
  const sdk = new OxySDK(createConfig());

  try {
    // Load multiple Parquet files
    await sdk.loadFile('data/sales.parquet', 'sales');
    await sdk.loadFile('data/customers.parquet', 'customers');

    // Query with SQL - supports JOINs across multiple tables
    const result = await sdk.query(`
      SELECT
        s.product,
        s.amount,
        c.name as customer_name,
        c.email
      FROM sales s
      JOIN customers c ON s.customer_id = c.id
      WHERE s.amount > 1000
      ORDER BY s.amount DESC
      LIMIT 10
    `);

    console.log('Columns:', result.columns);
    console.log('Rows:', result.rows);
    console.log('Total rows:', result.rowCount);

    return result;
  } finally {
    // Always cleanup
    await sdk.close();
  }
}
```

### 2. Load App Data Automatically

The SDK can automatically load all data files from an app and register them as tables:

```typescript
import { OxySDK, createConfig } from '@oxy-hq/sdk';

async function loadDashboard() {
  const sdk = new OxySDK(createConfig());

  try {
    // Loads all data from the app and registers tables
    // Table names match the keys in the data container
    const dataContainer = await sdk.loadAppData('dashboard.app.yml');

    console.log('Available tables:', Object.keys(dataContainer));

    // Query the loaded tables directly
    const sales = await sdk.query('SELECT * FROM sales LIMIT 10');
    const customers = await sdk.query('SELECT COUNT(*) as total FROM customers');

    return { sales, customers };
  } finally {
    await sdk.close();
  }
}
```

### 3. Convenience Methods

```typescript
import { OxySDK } from '@oxy-hq/sdk';

const sdk = new OxySDK({
  baseUrl: 'https://api.oxy.tech',
  apiKey: 'your-api-key',
  projectId: 'your-project-uuid'
});

// Load app data
await sdk.loadAppData('sales.app.yml');

// Get all rows from a table
const allSales = await sdk.getAll('sales', 100);

// Get schema information
const schema = await sdk.getSchema('sales');
console.log('Columns:', schema.columns);

// Get row count
const count = await sdk.count('sales');
console.log('Total rows:', count);

// Custom queries
const topProducts = await sdk.query(`
  SELECT product, SUM(amount) as revenue
  FROM sales
  GROUP BY product
  ORDER BY revenue DESC
  LIMIT 5
`);
```

---

## React Integration

### Option 1: Using OxyProvider (Recommended)

The SDK provides a React Context that handles initialization, loading states, and cleanup automatically.

```tsx
'use client'

import { OxyProvider, useOxy, createConfig } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

// Wrap your app with OxyProvider
export default function App() {
  return (
    <OxyProvider
      config={createConfig()}
      onReady={(sdk) => console.log('SDK ready:', sdk)}
      onError={(err) => console.error('SDK error:', err)}
    >
      <Dashboard />
    </OxyProvider>
  );
}

// Access SDK in child components
function Dashboard() {
  const { sdk, isLoading, error } = useOxy();
  const [sales, setSales] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadData() {
      // Load app data
      await sdk.loadAppData('dashboard.app.yml');

      // Query data
      const result = await sdk.query('SELECT * FROM sales LIMIT 100');
      setSales(result);
    }

    loadData().catch(console.error);
  }, [sdk]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="text-lg">Initializing SDK...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-50 p-4 rounded">
        <strong>Error:</strong> {error.message}
      </div>
    );
  }

  if (!sales) {
    return <div>Loading data...</div>;
  }

  return (
    <div>
      <h1 className="text-2xl font-bold mb-4">Sales Dashboard</h1>
      <DataTable data={sales} />
    </div>
  );
}

function DataTable({ data }: { data: { columns: string[], rows: unknown[][] } }) {
  return (
    <table className="w-full border-collapse">
      <thead>
        <tr>
          {data.columns.map(col => (
            <th key={col} className="border p-2 bg-gray-100">{col}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {data.rows.map((row, i) => (
          <tr key={i}>
            {row.map((cell, j) => (
              <td key={j} className="border p-2">{String(cell)}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

### Option 2: Custom Hook

Create a reusable hook for common data loading patterns:

```tsx
'use client'

import { useOxy } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

export function useOxyQuery(sql: string, deps: any[] = []) {
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
  }, [sdk, sql, ...deps]);

  return { data, loading, error };
}

// Usage in components
function SalesChart() {
  const { data, loading, error } = useOxyQuery(`
    SELECT DATE_TRUNC('month', date) as month, SUM(amount) as revenue
    FROM sales
    GROUP BY month
    ORDER BY month DESC
    LIMIT 12
  `);

  if (loading) return <div>Loading chart data...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return <LineChart data={data.rows} />;
}
```

### Option 3: Load App Data Hook

```tsx
'use client'

import { useOxy } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';
import type { DataContainer } from '@oxy-hq/sdk';

export function useOxyApp(appPath: string) {
  const { sdk } = useOxy();
  const [appData, setAppData] = useState<DataContainer | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

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
    if (!sdk || !appData) return;

    // Now query the loaded data
    sdk.query('SELECT COUNT(*) as total, SUM(amount) as revenue FROM sales')
      .then(result => setMetrics(result.rows[0]))
      .catch(console.error);
  }, [sdk, appData]);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <div>
      <h2>Available Tables:</h2>
      <ul>
        {Object.keys(appData).map(tableName => (
          <li key={tableName}>{tableName}</li>
        ))}
      </ul>

      {metrics && (
        <div>
          <p>Total Sales: {metrics.total}</p>
          <p>Revenue: ${metrics.revenue}</p>
        </div>
      )}
    </div>
  );
}
```

---

## Iframe & PostMessage Auth

For embedding in iframes (like v0.dev preview), the SDK supports automatic authentication via postMessage.

### Iframe Setup (Child - Your v0 App)

```tsx
'use client'

import { OxyProvider, useOxy } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

export default function App() {
  return (
    <OxyProvider
      useAsync={true}  // Enable async initialization
      config={{
        parentOrigin: 'https://v0.dev',  // Parent window origin
        projectId: 'your-project-uuid',
        baseUrl: 'https://api.oxy.tech'
      }}
      onReady={(sdk) => console.log('SDK authenticated via postMessage')}
      onError={(err) => console.error('Auth failed:', err)}
    >
      <EmbeddedDashboard />
    </OxyProvider>
  );
}

function EmbeddedDashboard() {
  const { sdk, isLoading, error } = useOxy();
  const [data, setData] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    // SDK is now authenticated and ready
    sdk.loadAppData('dashboard.app.yml')
      .then(() => sdk.query('SELECT * FROM metrics LIMIT 10'))
      .then(setData)
      .catch(console.error);
  }, [sdk]);

  if (isLoading) {
    return <div>Waiting for authentication...</div>;
  }

  if (error) {
    return <div>Authentication failed: {error.message}</div>;
  }

  return <div>{/* Render your data */}</div>;
}
```

### Parent Window Setup

If you're building the parent application that embeds v0 previews:

```typescript
// Parent window - listens for auth requests from iframe
window.addEventListener('message', (event) => {
  // Only respond to auth requests
  if (event.data.type !== 'OXY_AUTH_REQUEST') return;

  // Validate origin for security
  if (event.origin !== 'https://your-iframe-origin.com') return;

  // Send auth response
  event.source.postMessage({
    type: 'OXY_AUTH_RESPONSE',
    version: '1.0',
    requestId: event.data.requestId,
    apiKey: getUserApiKey(),  // Your function to get API key
    projectId: 'your-project-uuid',
    baseUrl: 'https://api.oxy.tech'
  }, event.origin);
});
```

### Alternative: Manual postMessage Auth

For more control over the authentication flow:

```tsx
'use client'

import { OxySDK } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

export default function App() {
  const [sdk, setSdk] = useState<OxySDK | null>(null);
  const [authStatus, setAuthStatus] = useState<'waiting' | 'success' | 'error'>('waiting');

  useEffect(() => {
    async function authenticate() {
      try {
        // This will automatically request auth from parent
        const sdkInstance = await OxySDK.create({
          parentOrigin: 'https://v0.dev',
          projectId: 'your-project-uuid',
          baseUrl: 'https://api.oxy.tech'
        });

        setSdk(sdkInstance);
        setAuthStatus('success');
      } catch (err) {
        console.error('Authentication failed:', err);
        setAuthStatus('error');
      }
    }

    authenticate();

    return () => {
      if (sdk) sdk.close();
    };
  }, []);

  if (authStatus === 'waiting') {
    return <div>Authenticating...</div>;
  }

  if (authStatus === 'error') {
    return <div>Authentication failed</div>;
  }

  return <Dashboard sdk={sdk} />;
}
```

---

## Complete Working Examples

### Example 1: Sales Dashboard

```tsx
'use client'

import { OxyProvider, useOxy, createConfig } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';

export default function SalesDashboard() {
  return (
    <OxyProvider config={createConfig()}>
      <DashboardContent />
    </OxyProvider>
  );
}

function DashboardContent() {
  const { sdk, isLoading, error } = useOxy();
  const [metrics, setMetrics] = useState(null);
  const [topProducts, setTopProducts] = useState(null);
  const [recentSales, setRecentSales] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadDashboard() {
      // Load app data
      await sdk.loadAppData('sales.app.yml');

      // Load metrics
      const metricsResult = await sdk.query(`
        SELECT
          COUNT(*) as total_sales,
          SUM(amount) as total_revenue,
          AVG(amount) as avg_sale,
          COUNT(DISTINCT customer_id) as unique_customers
        FROM sales
      `);
      setMetrics(metricsResult.rows[0]);

      // Load top products
      const productsResult = await sdk.query(`
        SELECT
          product,
          COUNT(*) as sales_count,
          SUM(amount) as revenue
        FROM sales
        GROUP BY product
        ORDER BY revenue DESC
        LIMIT 5
      `);
      setTopProducts(productsResult);

      // Load recent sales
      const recentResult = await sdk.query(`
        SELECT
          date,
          product,
          amount,
          customer_id
        FROM sales
        ORDER BY date DESC
        LIMIT 10
      `);
      setRecentSales(recentResult);
    }

    loadDashboard().catch(console.error);
  }, [sdk]);

  if (isLoading) return <LoadingSpinner />;
  if (error) return <ErrorMessage error={error} />;
  if (!metrics) return <div>Loading data...</div>;

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-3xl font-bold">Sales Dashboard</h1>

      {/* Metrics */}
      <div className="grid grid-cols-4 gap-4">
        <MetricCard title="Total Sales" value={metrics[0]} />
        <MetricCard title="Revenue" value={`$${Number(metrics[1]).toLocaleString()}`} />
        <MetricCard title="Avg Sale" value={`$${Number(metrics[2]).toFixed(2)}`} />
        <MetricCard title="Customers" value={metrics[3]} />
      </div>

      {/* Top Products */}
      {topProducts && (
        <div className="bg-white rounded-lg shadow p-6">
          <h2 className="text-xl font-semibold mb-4">Top Products</h2>
          <table className="w-full">
            <thead>
              <tr className="border-b">
                {topProducts.columns.map(col => (
                  <th key={col} className="text-left p-2">{col}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {topProducts.rows.map((row, i) => (
                <tr key={i} className="border-b">
                  {row.map((cell, j) => (
                    <td key={j} className="p-2">
                      {j === 2 ? `$${Number(cell).toLocaleString()}` : cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Recent Sales */}
      {recentSales && (
        <div className="bg-white rounded-lg shadow p-6">
          <h2 className="text-xl font-semibold mb-4">Recent Sales</h2>
          <table className="w-full">
            <thead>
              <tr className="border-b">
                {recentSales.columns.map(col => (
                  <th key={col} className="text-left p-2">{col}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {recentSales.rows.map((row, i) => (
                <tr key={i} className="border-b">
                  {row.map((cell, j) => (
                    <td key={j} className="p-2">{String(cell)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function MetricCard({ title, value }: { title: string, value: any }) {
  return (
    <div className="bg-white rounded-lg shadow p-4">
      <div className="text-sm text-gray-600">{title}</div>
      <div className="text-2xl font-bold mt-1">{value}</div>
    </div>
  );
}

function LoadingSpinner() {
  return (
    <div className="flex items-center justify-center h-screen">
      <div className="text-lg">Loading...</div>
    </div>
  );
}

function ErrorMessage({ error }: { error: Error }) {
  return (
    <div className="bg-red-50 border border-red-200 rounded p-4 m-4">
      <strong className="text-red-800">Error:</strong>{' '}
      <span className="text-red-600">{error.message}</span>
    </div>
  );
}
```

### Example 2: Data Explorer

```tsx
'use client'

import { OxyProvider, useOxy } from '@oxy-hq/sdk';
import { useState, useEffect } from 'react';
import type { DataContainer } from '@oxy-hq/sdk';

export default function DataExplorer() {
  return (
    <OxyProvider config={{
      baseUrl: 'https://api.oxy.tech',
      apiKey: process.env.NEXT_PUBLIC_OXY_API_KEY || '',
      projectId: process.env.NEXT_PUBLIC_OXY_PROJECT_ID || ''
    }}>
      <ExplorerContent />
    </OxyProvider>
  );
}

function ExplorerContent() {
  const { sdk, isLoading } = useOxy();
  const [appData, setAppData] = useState<DataContainer | null>(null);
  const [selectedTable, setSelectedTable] = useState<string>('');
  const [tableData, setTableData] = useState<any>(null);
  const [sqlQuery, setSqlQuery] = useState('');
  const [queryResult, setQueryResult] = useState<any>(null);

  useEffect(() => {
    if (!sdk) return;

    sdk.loadAppData('data.app.yml')
      .then(setAppData)
      .catch(console.error);
  }, [sdk]);

  const handleTableSelect = async (tableName: string) => {
    if (!sdk) return;

    setSelectedTable(tableName);
    const result = await sdk.getAll(tableName, 100);
    setTableData(result);
  };

  const handleQueryExecute = async () => {
    if (!sdk || !sqlQuery) return;

    try {
      const result = await sdk.query(sqlQuery);
      setQueryResult(result);
    } catch (err: any) {
      setQueryResult({ error: err.message });
    }
  };

  if (isLoading) return <div>Loading SDK...</div>;

  return (
    <div className="h-screen flex">
      {/* Sidebar - Table List */}
      <div className="w-64 bg-gray-100 p-4 overflow-y-auto">
        <h2 className="text-lg font-bold mb-4">Tables</h2>
        {appData && Object.keys(appData).map(tableName => (
          <button
            key={tableName}
            onClick={() => handleTableSelect(tableName)}
            className={`w-full text-left p-2 rounded mb-1 ${
              selectedTable === tableName
                ? 'bg-blue-500 text-white'
                : 'bg-white hover:bg-gray-200'
            }`}
          >
            {tableName}
          </button>
        ))}
      </div>

      {/* Main Content */}
      <div className="flex-1 p-6 space-y-4">
        {/* SQL Query Editor */}
        <div className="bg-white rounded-lg shadow p-4">
          <h2 className="text-lg font-semibold mb-2">SQL Query</h2>
          <textarea
            value={sqlQuery}
            onChange={(e) => setSqlQuery(e.target.value)}
            placeholder="SELECT * FROM table_name LIMIT 10"
            className="w-full h-32 p-2 border rounded font-mono text-sm"
          />
          <button
            onClick={handleQueryExecute}
            className="mt-2 px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
          >
            Execute Query
          </button>
        </div>

        {/* Query Results */}
        {queryResult && (
          <div className="bg-white rounded-lg shadow p-4">
            <h2 className="text-lg font-semibold mb-2">Results</h2>
            {queryResult.error ? (
              <div className="text-red-600">{queryResult.error}</div>
            ) : (
              <DataTable data={queryResult} />
            )}
          </div>
        )}

        {/* Table Preview */}
        {tableData && (
          <div className="bg-white rounded-lg shadow p-4">
            <h2 className="text-lg font-semibold mb-2">
              {selectedTable} (showing {tableData.rowCount} rows)
            </h2>
            <DataTable data={tableData} />
          </div>
        )}
      </div>
    </div>
  );
}

function DataTable({ data }: { data: { columns: string[], rows: unknown[][] } }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="bg-gray-100">
            {data.columns.map(col => (
              <th key={col} className="border p-2 text-left font-semibold">
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.rows.map((row, i) => (
            <tr key={i} className="hover:bg-gray-50">
              {row.map((cell, j) => (
                <td key={j} className="border p-2">
                  {cell === null ? (
                    <span className="text-gray-400 italic">null</span>
                  ) : (
                    String(cell)
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

### Example 3: Data Visualization

```tsx
'use client'

import { OxyProvider, useOxy } from '@oxy-hq/sdk';
import { useEffect, useState } from 'react';
import { LineChart, Line, BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend } from 'recharts';

export default function DataVisualization() {
  return (
    <OxyProvider config={{
      baseUrl: process.env.NEXT_PUBLIC_OXY_URL || '',
      apiKey: process.env.NEXT_PUBLIC_OXY_API_KEY || '',
      projectId: process.env.NEXT_PUBLIC_OXY_PROJECT_ID || ''
    }}>
      <ChartsContent />
    </OxyProvider>
  );
}

function ChartsContent() {
  const { sdk, isLoading } = useOxy();
  const [salesTrend, setSalesTrend] = useState([]);
  const [productPerformance, setProductPerformance] = useState([]);

  useEffect(() => {
    if (!sdk) return;

    async function loadChartData() {
      await sdk.loadAppData('sales.app.yml');

      // Sales trend over time
      const trendResult = await sdk.query(`
        SELECT
          DATE_TRUNC('month', date) as month,
          SUM(amount) as revenue,
          COUNT(*) as sales_count
        FROM sales
        GROUP BY month
        ORDER BY month
      `);

      setSalesTrend(
        trendResult.rows.map(row => ({
          month: new Date(row[0]).toLocaleDateString('en-US', { month: 'short', year: 'numeric' }),
          revenue: Number(row[1]),
          sales: Number(row[2])
        }))
      );

      // Product performance
      const productResult = await sdk.query(`
        SELECT
          product,
          SUM(amount) as revenue,
          COUNT(*) as units_sold
        FROM sales
        GROUP BY product
        ORDER BY revenue DESC
        LIMIT 10
      `);

      setProductPerformance(
        productResult.rows.map(row => ({
          product: row[0],
          revenue: Number(row[1]),
          units: Number(row[2])
        }))
      );
    }

    loadChartData().catch(console.error);
  }, [sdk]);

  if (isLoading) return <div>Loading...</div>;

  return (
    <div className="p-6 space-y-8">
      <h1 className="text-3xl font-bold">Sales Analytics</h1>

      {/* Sales Trend */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-xl font-semibold mb-4">Monthly Sales Trend</h2>
        {salesTrend.length > 0 && (
          <LineChart width={800} height={300} data={salesTrend}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="month" />
            <YAxis />
            <Tooltip />
            <Legend />
            <Line type="monotone" dataKey="revenue" stroke="#8884d8" name="Revenue ($)" />
            <Line type="monotone" dataKey="sales" stroke="#82ca9d" name="Sales Count" />
          </LineChart>
        )}
      </div>

      {/* Product Performance */}
      <div className="bg-white rounded-lg shadow p-6">
        <h2 className="text-xl font-semibold mb-4">Top Products by Revenue</h2>
        {productPerformance.length > 0 && (
          <BarChart width={800} height={400} data={productPerformance}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis dataKey="product" />
            <YAxis />
            <Tooltip />
            <Legend />
            <Bar dataKey="revenue" fill="#8884d8" name="Revenue ($)" />
            <Bar dataKey="units" fill="#82ca9d" name="Units Sold" />
          </BarChart>
        )}
      </div>
    </div>
  );
}
```

---

## Common Patterns

### Pattern 1: Lazy Loading with Pagination

```tsx
function PaginatedTable({ tableName }: { tableName: string }) {
  const { sdk } = useOxy();
  const [page, setPage] = useState(0);
  const [data, setData] = useState(null);
  const pageSize = 50;

  useEffect(() => {
    if (!sdk) return;

    sdk.query(`
      SELECT * FROM ${tableName}
      LIMIT ${pageSize}
      OFFSET ${page * pageSize}
    `).then(setData);
  }, [sdk, tableName, page]);

  return (
    <div>
      {data && <DataTable data={data} />}
      <div className="flex gap-2 mt-4">
        <button
          onClick={() => setPage(p => Math.max(0, p - 1))}
          disabled={page === 0}
        >
          Previous
        </button>
        <span>Page {page + 1}</span>
        <button onClick={() => setPage(p => p + 1)}>
          Next
        </button>
      </div>
    </div>
  );
}
```

### Pattern 2: Real-time Filtering

```tsx
function FilterableData() {
  const { sdk } = useOxy();
  const [minAmount, setMinAmount] = useState(0);
  const [product, setProduct] = useState('');
  const [data, setData] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    const query = `
      SELECT * FROM sales
      WHERE amount >= ${minAmount}
      ${product ? `AND product = '${product}'` : ''}
      ORDER BY date DESC
      LIMIT 100
    `;

    sdk.query(query).then(setData);
  }, [sdk, minAmount, product]);

  return (
    <div>
      <div className="mb-4 space-x-4">
        <input
          type="number"
          value={minAmount}
          onChange={(e) => setMinAmount(Number(e.target.value))}
          placeholder="Min amount"
          className="border p-2 rounded"
        />
        <input
          type="text"
          value={product}
          onChange={(e) => setProduct(e.target.value)}
          placeholder="Product name"
          className="border p-2 rounded"
        />
      </div>
      {data && <DataTable data={data} />}
    </div>
  );
}
```

### Pattern 3: Data Aggregation

```tsx
function AggregatedMetrics() {
  const { sdk } = useOxy();
  const [timeRange, setTimeRange] = useState('month');
  const [aggregated, setAggregated] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    sdk.query(`
      SELECT
        DATE_TRUNC('${timeRange}', date) as period,
        COUNT(*) as count,
        SUM(amount) as total,
        AVG(amount) as average,
        MIN(amount) as min,
        MAX(amount) as max
      FROM sales
      GROUP BY period
      ORDER BY period DESC
    `).then(setAggregated);
  }, [sdk, timeRange]);

  return (
    <div>
      <select value={timeRange} onChange={(e) => setTimeRange(e.target.value)}>
        <option value="day">Daily</option>
        <option value="week">Weekly</option>
        <option value="month">Monthly</option>
        <option value="year">Yearly</option>
      </select>
      {aggregated && <DataTable data={aggregated} />}
    </div>
  );
}
```

### Pattern 4: Multi-Table Joins

```tsx
function CustomerSalesReport() {
  const { sdk } = useOxy();
  const [report, setReport] = useState(null);

  useEffect(() => {
    if (!sdk) return;

    async function loadReport() {
      // Load both tables
      await sdk.loadFiles([
        { filePath: 'data/sales.parquet', tableName: 'sales' },
        { filePath: 'data/customers.parquet', tableName: 'customers' }
      ]);

      // Join and analyze
      const result = await sdk.query(`
        SELECT
          c.name,
          c.email,
          COUNT(s.id) as purchase_count,
          SUM(s.amount) as total_spent,
          AVG(s.amount) as avg_purchase,
          MAX(s.date) as last_purchase
        FROM customers c
        LEFT JOIN sales s ON c.id = s.customer_id
        GROUP BY c.id, c.name, c.email
        HAVING COUNT(s.id) > 0
        ORDER BY total_spent DESC
        LIMIT 50
      `);

      setReport(result);
    }

    loadReport();
  }, [sdk]);

  return report ? <DataTable data={report} /> : <div>Loading...</div>;
}
```

---

## Troubleshooting

### Issue: "process is not defined"

**Cause**: Browser environment doesn't have Node.js `process` object.

**Solution**: The SDK handles this automatically. For Vite projects, use `VITE_` prefix:

```env
VITE_OXY_URL=https://api.oxy.tech
VITE_OXY_API_KEY=your-key
VITE_OXY_PROJECT_ID=your-id
```

### Issue: "SDK not ready" or "sdk is null"

**Cause**: Component trying to use SDK before initialization completes.

**Solution**: Always check SDK is available:

```tsx
const { sdk, isLoading } = useOxy();

if (isLoading) return <div>Loading...</div>;
if (!sdk) return <div>SDK not available</div>;

// Safe to use sdk here
```

### Issue: PostMessage authentication timeout

**Cause**: Parent window not responding to auth requests.

**Solution**:
1. Verify `parentOrigin` matches parent window origin exactly
2. Ensure parent window has message listener set up
3. Check browser console for postMessage errors
4. Increase timeout: `config={{ parentOrigin: '...', timeout: 10000 }}`

### Issue: Table not found in query

**Cause**: Trying to query before data is loaded.

**Solution**: Always await `loadAppData` or `loadFile` before querying:

```tsx
await sdk.loadAppData('app.yml');
// Now safe to query
const result = await sdk.query('SELECT * FROM table_name');
```

### Issue: Memory leaks

**Cause**: Not cleaning up SDK resources.

**Solution**: OxyProvider handles cleanup automatically. If using SDK directly:

```tsx
useEffect(() => {
  const sdk = new OxySDK(config);

  // ... use sdk

  return () => {
    sdk.close(); // Cleanup on unmount
  };
}, []);
```

### Issue: CORS errors

**Cause**: API doesn't allow requests from your origin.

**Solution**: Contact Oxy support to whitelist your domain, or use server-side API routes.

### Issue: Query performance is slow

**Solutions**:
1. Add `LIMIT` clause to queries
2. Use indexes if available
3. Load only needed columns: `SELECT specific, columns FROM table`
4. Consider pagination for large datasets

---

## Best Practices

1. **Always use OxyProvider** - Let it handle initialization and cleanup
2. **Check SDK availability** - Always check `isLoading` and `sdk` before use
3. **Load data once** - Load app data in parent component, query in children
4. **Cleanup resources** - OxyProvider does this automatically
5. **Handle errors** - Always check `error` from `useOxy()`
6. **Use TypeScript** - Get type safety for better DX
7. **Limit query results** - Use `LIMIT` to avoid loading too much data
8. **Cache queries** - Use React state to avoid re-querying
9. **Security** - Never expose API keys in client code for production
10. **PostMessage origin** - Always specify exact origin, never use `'*'` in production

---

## API Quick Reference

### OxySDK Methods

```typescript
// Initialization
const sdk = new OxySDK(config)
const sdk = await OxySDK.create(config)

// Load data
await sdk.loadFile(filePath, tableName)
await sdk.loadFiles([{ filePath, tableName }, ...])
await sdk.loadAppData(appPath)

// Query
await sdk.query(sql)
await sdk.getAll(tableName, limit?)
await sdk.getSchema(tableName)
await sdk.count(tableName)

// Access underlying clients
sdk.getClient()  // OxyClient
sdk.getReader()  // ParquetReader

// Cleanup
await sdk.close()
```

### React Hooks

```typescript
// Provider
<OxyProvider config={config} useAsync={boolean} onReady={fn} onError={fn}>

// Hooks
const { sdk, isLoading, error } = useOxy()
const sdk = useOxySDK()  // Throws if not ready
```

### Config

```typescript
interface OxyConfig {
  baseUrl: string              // API URL
  apiKey?: string              // API key (optional for iframe)
  projectId: string            // Project UUID
  branch?: string              // Branch name
  parentOrigin?: string        // For postMessage auth
  disableAutoAuth?: boolean    // Disable auto iframe auth
  timeout?: number             // Request timeout (ms)
}
```

---

## Support

- **GitHub**: [dataframehq/oxy-internal](https://github.com/dataframehq/oxy-internal)
- **Documentation**: See [README.md](../README.md)
- **Issues**: [GitHub Issues](https://github.com/dataframehq/oxy-internal/issues)
