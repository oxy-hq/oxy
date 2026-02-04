import * as duckdb from "@duckdb/duckdb-wasm";
import eh_worker from "@duckdb/duckdb-wasm/dist/duckdb-browser-eh.worker.js?url";
import mvp_worker from "@duckdb/duckdb-wasm/dist/duckdb-browser-mvp.worker.js?url";
import duckdb_wasm_eh from "@duckdb/duckdb-wasm/dist/duckdb-eh.wasm?url";
import duckdb_wasm from "@duckdb/duckdb-wasm/dist/duckdb-mvp.wasm?url";

const isLocalhost = () => {
  if (typeof window === "undefined") return false;
  return window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1";
};

let duckDB: duckdb.AsyncDuckDB = null!;
let initPromise: Promise<void> | null = null;

const init = async () => {
  if (duckDB) return;
  if (!initPromise) {
    initPromise = (async () => {
      console.debug("Initializing DuckDB");
      let bundle: duckdb.DuckDBBundle;
      let worker: Worker;
      if (isLocalhost()) {
        // Use manual bundles for localhost
        bundle = await duckdb.selectBundle({
          mvp: {
            mainModule: duckdb_wasm,
            mainWorker: mvp_worker
          },
          eh: {
            mainModule: duckdb_wasm_eh,
            mainWorker: eh_worker
          }
        });
        worker = new Worker(bundle.mainWorker!);
      } else {
        // Use CDN bundles for cloud
        const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
        bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);
        const worker_url = URL.createObjectURL(
          new Blob([`importScripts("${bundle.mainWorker!}");`], {
            type: "text/javascript"
          })
        );
        worker = new Worker(worker_url);
        // Clean up after instantiation
        const origInstantiate = duckdb.AsyncDuckDB.prototype.instantiate;
        await origInstantiate.call(
          // eslint-disable-next-line sonarjs/no-nested-assignment
          (duckDB = new duckdb.AsyncDuckDB(new duckdb.ConsoleLogger(), worker)),
          bundle.mainModule,
          bundle.pthreadWorker
        );
        URL.revokeObjectURL(worker_url);
        return;
      }
      const logger = new duckdb.ConsoleLogger();
      duckDB = new duckdb.AsyncDuckDB(logger, worker);
      await duckDB.instantiate(bundle.mainModule, bundle.pthreadWorker);
    })();
  }
  return initPromise;
};

export const getDuckDB = async () => {
  if (!duckDB) {
    await init();
  }
  return duckDB;
};

/**
 * Register a Parquet file from the API endpoint with authentication
 * @param filePath - The file path to register (e.g., result file path from API)
 * @param projectId - The project ID for authentication
 * @param branchName - The branch name for the request
 * @returns The registered table name in DuckDB
 */
export const registerAuthenticatedParquetFile = async (
  filePath: string,
  projectId: string,
  branchName: string
): Promise<string> => {
  const db = await getDuckDB();

  // Base64 encode the file path to create a valid table name
  const tableName = btoa(filePath).replace(/[^a-zA-Z0-9]/g, "_");

  // Fetch the Parquet file from the API
  const { apiClient } = await import("@/services/api/axios");

  const response = await apiClient.get(`/${projectId}/results/files/${filePath}`, {
    responseType: "arraybuffer",
    params: { branch: branchName }
  });

  const fileData = new Uint8Array(response.data);

  const conn = await db.connect();

  // Drop table if it exists to ensure fresh data
  try {
    await conn.query(`DROP TABLE IF EXISTS "${tableName}"`);
    console.log(`Dropped existing table "${tableName}" if it existed`);
  } catch (e) {
    console.warn("Error dropping table:", e);
  }

  // Insert Parquet data directly into DuckDB
  try {
    // Register the Parquet data as a file in DuckDB's virtual filesystem
    await db.registerFileBuffer(`${tableName}.parquet`, fileData);

    // Create a table from the Parquet file
    await conn.query(
      `CREATE TABLE "${tableName}" AS SELECT * FROM parquet_scan('${tableName}.parquet')`
    );

    // Verify the table was created
    const verifyResult = await conn.query(`SELECT COUNT(*) as cnt FROM "${tableName}"`);
    const count = verifyResult.toArray()[0].cnt;
    console.log(`Successfully loaded Parquet data into table "${tableName}" with ${count} rows`);
  } catch (e) {
    console.error("Error loading Parquet data:", e);
    await conn.close();
    throw e;
  }

  await conn.close();
  return tableName;
};
