import * as duckdb from "@duckdb/duckdb-wasm";
import duckdb_wasm from "@duckdb/duckdb-wasm/dist/duckdb-mvp.wasm?url";
import mvp_worker from "@duckdb/duckdb-wasm/dist/duckdb-browser-mvp.worker.js?url";
import duckdb_wasm_eh from "@duckdb/duckdb-wasm/dist/duckdb-eh.wasm?url";
import eh_worker from "@duckdb/duckdb-wasm/dist/duckdb-browser-eh.worker.js?url";

const isLocalhost = () => {
  if (typeof window === "undefined") return false;
  return (
    window.location.hostname === "localhost" ||
    window.location.hostname === "127.0.0.1"
  );
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
            mainWorker: mvp_worker,
          },
          eh: {
            mainModule: duckdb_wasm_eh,
            mainWorker: eh_worker,
          },
        });
        worker = new Worker(bundle.mainWorker!);
      } else {
        // Use CDN bundles for cloud
        const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
        bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);
        const worker_url = URL.createObjectURL(
          new Blob([`importScripts("${bundle.mainWorker!}");`], {
            type: "text/javascript",
          }),
        );
        worker = new Worker(worker_url);
        // Clean up after instantiation
        const origInstantiate = duckdb.AsyncDuckDB.prototype.instantiate;
        await origInstantiate.call(
          // eslint-disable-next-line sonarjs/no-nested-assignment
          (duckDB = new duckdb.AsyncDuckDB(new duckdb.ConsoleLogger(), worker)),
          bundle.mainModule,
          bundle.pthreadWorker,
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
