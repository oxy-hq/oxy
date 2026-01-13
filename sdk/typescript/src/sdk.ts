import { OxyClient } from "./client";
import { OxyConfig, createConfigAsync } from "./config";
import { ParquetReader, QueryResult } from "./parquet";
import { DataContainer } from "./types";

/**
 * OxySDK provides a unified interface for fetching data from Oxy and querying it with SQL.
 * It combines OxyClient (for API calls) and ParquetReader (for SQL queries) into a single,
 * easy-to-use interface.
 *
 * @example
 * ```typescript
 * // Create SDK instance
 * const sdk = new OxySDK({ apiKey: 'your-key', projectId: 'your-project' });
 *
 * // Load a parquet file and query it
 * await sdk.loadFile('data/sales.parquet', 'sales');
 * const result = await sdk.query('SELECT * FROM sales WHERE amount > 1000');
 * console.log(result.rows);
 *
 * // Clean up when done
 * await sdk.close();
 * ```
 */
export class OxySDK {
  private client: OxyClient;
  private reader: ParquetReader;

  constructor(config: OxyConfig) {
    this.client = new OxyClient(config);
    this.reader = new ParquetReader();
  }

  /**
   * Creates an OxySDK instance asynchronously with support for postMessage authentication
   *
   * @param config - Optional configuration overrides
   * @returns Promise resolving to OxySDK instance
   *
   * @example
   * ```typescript
   * // In an iframe - automatic postMessage auth
   * const sdk = await OxySDK.create({
   *   parentOrigin: 'https://app.example.com',
   *   projectId: 'my-project-id'
   * });
   * ```
   */
  static async create(config?: Partial<OxyConfig>): Promise<OxySDK> {
    const resolvedConfig = await createConfigAsync(config);
    return new OxySDK(resolvedConfig);
  }

  /**
   * Load a Parquet file from Oxy and register it for SQL queries
   *
   * @param filePath - Path to the parquet file in the app state directory
   * @param tableName - Name to use for the table in SQL queries
   *
   * @example
   * ```typescript
   * await sdk.loadFile('data/sales.parquet', 'sales');
   * await sdk.loadFile('data/customers.parquet', 'customers');
   *
   * const result = await sdk.query(`
   *   SELECT s.*, c.name
   *   FROM sales s
   *   JOIN customers c ON s.customer_id = c.id
   * `);
   * ```
   */
  async loadFile(filePath: string, tableName: string): Promise<void> {
    const blob = await this.client.getFile(filePath);
    await this.reader.registerParquet(blob, tableName);
  }

  /**
   * Load multiple Parquet files at once
   *
   * @param files - Array of file paths and table names
   *
   * @example
   * ```typescript
   * await sdk.loadFiles([
   *   { filePath: 'data/sales.parquet', tableName: 'sales' },
   *   { filePath: 'data/customers.parquet', tableName: 'customers' },
   *   { filePath: 'data/products.parquet', tableName: 'products' }
   * ]);
   *
   * const result = await sdk.query('SELECT * FROM sales');
   * ```
   */
  async loadFiles(
    files: Array<{ filePath: string; tableName: string }>,
  ): Promise<void> {
    for (const file of files) {
      await this.loadFile(file.filePath, file.tableName);
    }
  }

  /**
   * Load all data from an app's data container
   *
   * This fetches the app's data and registers all parquet files using their container keys as table names.
   *
   * @param appPath - Path to the app file
   * @returns DataContainer with file references
   *
   * @example
   * ```typescript
   * // If app has data: { sales: { file_path: 'data/sales.parquet' } }
   * const data = await sdk.loadAppData('dashboard.app.yml');
   * // Now you can query the 'sales' table
   * const result = await sdk.query('SELECT * FROM sales LIMIT 10');
   * ```
   */
  async loadAppData(appPath: string): Promise<DataContainer | null> {
    const appDataResponse = await this.client.getAppData(appPath);

    if (appDataResponse.error) {
      throw new Error(`Failed to load app data: ${appDataResponse.error}`);
    }

    if (!appDataResponse.data) {
      return null;
    }

    // Load each file in the data container
    const loadPromises = Object.entries(appDataResponse.data).map(
      async ([tableName, fileRef]) => {
        await this.loadFile(fileRef.file_path, tableName);
      },
    );

    await Promise.all(loadPromises);

    return appDataResponse.data;
  }

  /**
   * Execute a SQL query against loaded data
   *
   * @param sql - SQL query to execute
   * @returns Query result with columns and rows
   *
   * @example
   * ```typescript
   * await sdk.loadFile('data/sales.parquet', 'sales');
   *
   * const result = await sdk.query('SELECT product, SUM(amount) as total FROM sales GROUP BY product');
   * console.log(result.columns); // ['product', 'total']
   * console.log(result.rows);    // [['Product A', 1000], ['Product B', 2000]]
   * console.log(result.rowCount); // 2
   * ```
   */
  async query(sql: string): Promise<QueryResult> {
    return this.reader.query(sql);
  }

  /**
   * Get all data from a loaded table
   *
   * @param tableName - Name of the table
   * @param limit - Maximum number of rows (optional)
   * @returns Query result
   *
   * @example
   * ```typescript
   * await sdk.loadFile('data/sales.parquet', 'sales');
   * const allData = await sdk.getAll('sales');
   * const first100 = await sdk.getAll('sales', 100);
   * ```
   */
  async getAll(tableName: string, limit?: number): Promise<QueryResult> {
    return this.reader.getAll(tableName, limit);
  }

  /**
   * Get schema information for a loaded table
   *
   * @param tableName - Name of the table
   * @returns Schema information
   *
   * @example
   * ```typescript
   * await sdk.loadFile('data/sales.parquet', 'sales');
   * const schema = await sdk.getSchema('sales');
   * console.log(schema.columns); // ['column_name', 'column_type', ...]
   * console.log(schema.rows);    // [['id', 'INTEGER'], ['name', 'VARCHAR'], ...]
   * ```
   */
  async getSchema(tableName: string): Promise<QueryResult> {
    return this.reader.getSchema(tableName);
  }

  /**
   * Get row count for a loaded table
   *
   * @param tableName - Name of the table
   * @returns Number of rows
   *
   * @example
   * ```typescript
   * await sdk.loadFile('data/sales.parquet', 'sales');
   * const count = await sdk.count('sales');
   * console.log(`Total rows: ${count}`);
   * ```
   */
  async count(tableName: string): Promise<number> {
    return this.reader.count(tableName);
  }

  /**
   * Get direct access to the underlying OxyClient
   *
   * Useful for advanced operations like listing apps, getting displays, etc.
   *
   * @returns The OxyClient instance
   *
   * @example
   * ```typescript
   * const apps = await sdk.getClient().listApps();
   * const displays = await sdk.getClient().getDisplays('my-app.app.yml');
   * ```
   */
  getClient(): OxyClient {
    return this.client;
  }

  /**
   * Get direct access to the underlying ParquetReader
   *
   * Useful for advanced operations like registering blobs directly.
   *
   * @returns The ParquetReader instance
   *
   * @example
   * ```typescript
   * const myBlob = new Blob([parquetData]);
   * await sdk.getReader().registerParquet(myBlob, 'mydata');
   * ```
   */
  getReader(): ParquetReader {
    return this.reader;
  }

  /**
   * Close and cleanup all resources
   *
   * This clears all loaded data and releases resources. Call this when you're done with the SDK.
   *
   * @example
   * ```typescript
   * const sdk = new OxySDK({ apiKey: 'key', projectId: 'project' });
   * await sdk.loadFile('data/sales.parquet', 'sales');
   * const result = await sdk.query('SELECT * FROM sales');
   * await sdk.close(); // Clean up
   * ```
   */
  async close(): Promise<void> {
    await this.reader.close();
  }
}
