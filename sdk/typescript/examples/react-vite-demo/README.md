# Oxy SDK React + Vite Demo

A modern, interactive demo application showcasing the Oxy TypeScript SDK with React and Vite.

## Features

- Configure SDK connection with an easy-to-use form
- List all apps in your Oxy project
- View and run app data
- Preview parquet data files with interactive tables
- Beautiful, responsive UI with collapsible data previews
- Fast development with Vite HMR
- Full TypeScript support

## Prerequisites

Before running this demo, make sure you have:

1. Node.js 18+ installed
2. The Oxy SDK built (see instructions below)
3. Your Oxy credentials:
   - API URL (e.g., `https://api.oxy.tech`)
   - API Key
   - Project ID

## Quick Start

### 1. Build the Oxy SDK

First, build the SDK from the parent directory:

```bash
# From the sdk/typescript directory
cd ../..
pnpm install
pnpm build
```

### 2. Install Demo Dependencies

```bash
# Navigate to the demo directory
cd examples/react-vite-demo

# Install dependencies (use --ignore-workspace since this is not part of the workspace)
pnpm install --ignore-workspace
```

### 3. Run the Development Server

```bash
pnpm dev
```

This will start the Vite development server at [http://localhost:3000](http://localhost:3000).

### 4. Use the Demo

1. Open [http://localhost:3000](http://localhost:3000) in your browser
2. Fill in your Oxy credentials in the configuration form
3. Click "Connect to Oxy"
4. Click "List Apps" to see all available apps
5. Click on an app to view its data
6. Click "Run App" to execute the app and get fresh data

## Available Scripts

```bash
# Start development server with hot reload
pnpm dev

# Build for production
pnpm build

# Preview production build
pnpm preview

# Type check without building
pnpm typecheck
```

## Project Structure

```
react-vite-demo/
├── src/
│   ├── components/
│   │   ├── ConfigForm.tsx      # SDK configuration form
│   │   ├── AppList.tsx          # List of apps
│   │   └── AppDataView.tsx      # Display app data
│   ├── App.tsx                  # Main application component
│   ├── App.css                  # Application styles
│   ├── main.tsx                 # Entry point
│   └── index.css                # Global styles
├── index.html                   # HTML template
├── package.json                 # Dependencies and scripts
├── tsconfig.json                # TypeScript configuration
├── vite.config.ts               # Vite configuration
└── README.md                    # This file
```

## Using the SDK

This demo shows how to use the main SDK features:

### Initialize Client

```typescript
import { OxyClient } from "@oxy-hq/sdk";

const client = new OxyClient({
  url: "https://api.oxy.tech",
  apiKey: "your-api-key",
  projectId: "your-project-id",
  branch: "main", // optional
});
```

### List Apps

```typescript
const apps = await client.listApps();
console.log(apps); // [{ name: '...', path: '...' }, ...]
```

### Get App Data (Cached)

```typescript
const result = await client.getAppData("dashboard.app.yml");
if (!result.error) {
  console.log(result.data);
}
```

### Run App (Fresh Data)

```typescript
const result = await client.runApp("dashboard.app.yml");
if (!result.error) {
  console.log(result.data);
}
```

### Preview Parquet Data

```typescript
// Get table data from a parquet file
const tableData = await client.getTableData("data/sales.parquet", 100);
console.log(tableData.columns); // ['id', 'product', 'sales', ...]
console.log(tableData.rows); // [[1, 'Widget', 1000], ...]
console.log(tableData.total_rows); // 100
```

The demo includes an interactive modal preview feature that allows you to:

- Click the "👁 Preview" button on any dataset card to open a modal
- View the first 100 rows of parquet data in a scrollable table
- Toggle fullscreen mode for better data viewing (click fullscreen button or press 'F')
- Close the modal by clicking the X button, clicking outside, or pressing ESC
- In fullscreen mode, ESC exits fullscreen first, then closes the modal

## Customization

Feel free to modify this demo to suit your needs:

- Customize the number of rows shown in previews
- Integrate with your own UI component library
- Add data visualization components (charts, graphs)
- Implement file download functionality
- Add display configuration viewing
- Add SQL query capabilities using the ParquetReader

## Troubleshooting

### SDK not found

Make sure you've built the SDK first:

```bash
cd ../.. && pnpm build
```

### Module resolution errors

Try clearing node_modules and reinstalling:

```bash
rm -rf node_modules pnpm-lock.yaml
pnpm install
```

### CORS errors

If you encounter CORS errors when connecting to your Oxy instance, make sure your API server is configured to allow requests from `http://localhost:3000/api`.

## Learn More

- [Oxy SDK Documentation](../../README.md)
- [Vite Documentation](https://vitejs.dev/)
- [React Documentation](https://react.dev/)

## License

MIT
