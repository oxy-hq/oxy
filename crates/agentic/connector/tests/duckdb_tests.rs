//! Integration tests for the DuckDB connector.
//!
//! Migrated from the removed `agentic-connector-duckdb` crate.
//!
//! Non-file tests use `DuckDbConnector::new(conn)` with data pre-loaded into
//! the connection before handing it to the connector.  File-based tests use a
//! `tempfile` directory and write Parquet/CSV there first.

#[cfg(feature = "duckdb")]
mod duckdb {
    use agentic_connector::{
        ConnectorError, DatabaseConnector, DuckDbConnection, DuckDbConnector, LoadStrategy,
    };
    use agentic_core::result::CellValue;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Build a connector with a simple `sales` table pre-populated.
    ///
    /// Schema: id INTEGER, name TEXT, amount DOUBLE, tag TEXT
    ///   (row 3 has tag = NULL)
    fn make_sales_connector() -> DuckDbConnector {
        let conn = DuckDbConnection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sales (id INTEGER, name TEXT, amount DOUBLE, tag TEXT);
             INSERT INTO sales VALUES
                 (1, 'alpha', 10.0, 'a'),
                 (2, 'beta',  20.0, 'b'),
                 (3, 'gamma', 30.0, NULL);",
        )
        .unwrap();
        DuckDbConnector::new(conn)
    }

    // ── execute_query ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn basic_query() {
        let c = make_sales_connector();
        let res = c
            .execute_query("SELECT * FROM sales ORDER BY id", 100)
            .await
            .unwrap();

        assert_eq!(res.result.total_row_count, 3);
        assert_eq!(res.result.rows.len(), 3);
        assert_eq!(res.result.columns, ["id", "name", "amount", "tag"]);
        assert!(!res.result.truncated);

        let row0 = &res.result.rows[0].0;
        assert_eq!(row0[0], CellValue::Number(1.0));
        assert_eq!(row0[1], CellValue::Text("alpha".into()));
        assert_eq!(row0[2], CellValue::Number(10.0));
        assert_eq!(row0[3], CellValue::Text("a".into()));
    }

    #[tokio::test]
    async fn truncation() {
        let c = make_sales_connector();
        let res = c.execute_query("SELECT * FROM sales", 2).await.unwrap();

        assert_eq!(res.result.total_row_count, 3);
        assert_eq!(res.result.rows.len(), 2);
        assert!(res.result.truncated);
    }

    #[tokio::test]
    async fn empty_result() {
        let c = make_sales_connector();
        let res = c
            .execute_query("SELECT * FROM sales WHERE 1 = 0", 100)
            .await
            .unwrap();

        assert_eq!(res.result.total_row_count, 0);
        assert!(res.result.rows.is_empty());
        assert!(!res.result.truncated);
        assert_eq!(res.summary.row_count, 0);
    }

    #[tokio::test]
    async fn stats_correctness() {
        let c = make_sales_connector();
        let res = c.execute_query("SELECT * FROM sales", 100).await.unwrap();

        let amount = res
            .summary
            .columns
            .iter()
            .find(|s| s.name == "amount")
            .unwrap();
        assert_eq!(amount.null_count, 0);
        assert_eq!(amount.distinct_count, Some(3));
        assert_eq!(amount.min, Some(CellValue::Number(10.0)));
        assert_eq!(amount.max, Some(CellValue::Number(30.0)));
        assert!((amount.mean.unwrap() - 20.0).abs() < 1e-9);
        let expected_std = f64::sqrt(((10f64 - 20.0).powi(2) + 0.0 + (30f64 - 20.0).powi(2)) / 3.0);
        assert!((amount.std_dev.unwrap() - expected_std).abs() < 1e-6);

        let tag = res
            .summary
            .columns
            .iter()
            .find(|s| s.name == "tag")
            .unwrap();
        assert_eq!(tag.null_count, 1);
        assert_eq!(tag.distinct_count, Some(2));
        assert!(tag.mean.is_none());
        assert!(tag.std_dev.is_none());
    }

    #[tokio::test]
    async fn sql_error() {
        let c = DuckDbConnector::in_memory().unwrap();
        let err = c
            .execute_query("SELECT * FROM nonexistent_table_xyz", 100)
            .await
            .unwrap_err();
        assert!(matches!(err, ConnectorError::QueryFailed { .. }));
    }

    #[tokio::test]
    async fn dialect_is_duckdb() {
        use agentic_connector::SqlDialect;
        let c = DuckDbConnector::in_memory().unwrap();
        assert_eq!(c.dialect(), SqlDialect::DuckDb);
        assert_eq!(c.dialect().as_str(), "DuckDB");
    }

    // ── file loading ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn from_directory_parquet() {
        let dir = tempfile::tempdir().unwrap();
        let parquet_path = dir.path().join("orders.parquet");

        let wconn = DuckDbConnection::open_in_memory().unwrap();
        wconn
            .execute_batch(&format!(
                "COPY (SELECT 1 AS order_id, 100.0 AS total \
                       UNION ALL SELECT 2 AS order_id, 200.0 AS total) \
                 TO '{}' (FORMAT PARQUET)",
                parquet_path.display()
            ))
            .unwrap();

        let c = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View).unwrap();
        let res = c
            .execute_query("SELECT * FROM orders ORDER BY order_id", 100)
            .await
            .unwrap();

        assert_eq!(res.result.total_row_count, 2);
        assert_eq!(res.result.columns, ["order_id", "total"]);
    }

    #[tokio::test]
    async fn from_directory_csv() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("products.csv"),
            "product_id,price\n1,9.99\n2,19.99\n3,29.99\n",
        )
        .unwrap();

        let c = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View).unwrap();
        let res = c
            .execute_query("SELECT * FROM products ORDER BY product_id", 100)
            .await
            .unwrap();

        assert_eq!(res.result.total_row_count, 3);
        assert_eq!(res.result.columns, ["product_id", "price"]);
    }

    #[tokio::test]
    async fn view_vs_materialized() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("items.csv"), "id,val\n1,10\n2,20\n").unwrap();

        let sql = "SELECT * FROM items ORDER BY id";
        let view_res = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View)
            .unwrap()
            .execute_query(sql, 100)
            .await
            .unwrap();
        let mat_res = DuckDbConnector::from_directory(dir.path(), LoadStrategy::Materialized)
            .unwrap()
            .execute_query(sql, 100)
            .await
            .unwrap();

        assert_eq!(
            view_res.result.total_row_count,
            mat_res.result.total_row_count
        );
        assert_eq!(view_res.result.columns, mat_res.result.columns);
        assert_eq!(view_res.result.rows.len(), mat_res.result.rows.len());
    }

    #[tokio::test]
    async fn loaded_tables_metadata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("events.csv"),
            "event_id,name\n1,click\n2,view\n",
        )
        .unwrap();

        let c = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View).unwrap();
        let tables = c.loaded_tables();

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "events");
        assert!(tables[0].columns.iter().any(|(n, _)| n == "event_id"));
        assert!(tables[0].columns.iter().any(|(n, _)| n == "name"));
    }

    #[tokio::test]
    async fn parquet_preferred_over_csv_on_collision() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("data.csv"), "id,src\n1,csv\n").unwrap();
        let wconn = DuckDbConnection::open_in_memory().unwrap();
        wconn
            .execute_batch(&format!(
                "COPY (SELECT 2 AS id, 'parquet' AS src) TO '{}' (FORMAT PARQUET)",
                dir.path().join("data.parquet").display()
            ))
            .unwrap();

        let c = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View).unwrap();
        assert_eq!(
            c.loaded_tables().len(),
            1,
            "collision must yield exactly one table"
        );

        let res = c.execute_query("SELECT src FROM data", 10).await.unwrap();
        assert_eq!(
            res.result.rows[0].0[0],
            CellValue::Text("parquet".into()),
            "Parquet must win over CSV on stem collision"
        );
    }

    #[tokio::test]
    async fn from_files_per_file_strategy() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("small.csv");
        std::fs::write(&csv_path, "x\n1\n2\n3\n").unwrap();

        let c = DuckDbConnector::from_files(&[(&csv_path, LoadStrategy::Materialized)]).unwrap();
        let res = c
            .execute_query("SELECT * FROM small ORDER BY x", 100)
            .await
            .unwrap();

        assert_eq!(res.result.total_row_count, 3);
        assert_eq!(res.result.columns, ["x"]);
    }

    // ── introspect_schema ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn introspect_schema_finds_tables_and_columns() {
        let c = make_sales_connector();
        let info = c
            .introspect_schema()
            .expect("introspect_schema must succeed");

        let names: Vec<&str> = info.tables.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"sales"),
            "sales table must appear: {names:?}"
        );

        let sales = info.tables.iter().find(|t| t.name == "sales").unwrap();
        let col_names: Vec<&str> = sales.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"id"), "{col_names:?}");
        assert!(col_names.contains(&"name"), "{col_names:?}");
        assert!(col_names.contains(&"amount"), "{col_names:?}");
        assert!(col_names.contains(&"tag"), "{col_names:?}");
    }

    #[tokio::test]
    async fn introspect_schema_computes_min_max_for_numeric_column() {
        let c = make_sales_connector();
        let info = c.introspect_schema().unwrap();

        let sales = info.tables.iter().find(|t| t.name == "sales").unwrap();
        let amount_col = sales.columns.iter().find(|c| c.name == "amount").unwrap();

        assert_eq!(amount_col.min, Some(CellValue::Number(10.0)));
        assert_eq!(amount_col.max, Some(CellValue::Number(30.0)));
    }

    #[tokio::test]
    async fn introspect_schema_collects_sample_values() {
        let c = make_sales_connector();
        let info = c.introspect_schema().unwrap();

        let sales = info.tables.iter().find(|t| t.name == "sales").unwrap();
        let name_col = sales.columns.iter().find(|c| c.name == "name").unwrap();

        assert!(
            !name_col.sample_values.is_empty(),
            "name must have sample values"
        );
        let texts: Vec<_> = name_col
            .sample_values
            .iter()
            .filter_map(|v| {
                if let CellValue::Text(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            texts.iter().any(|s| ["alpha", "beta", "gamma"].contains(s)),
            "expected alpha/beta/gamma in samples: {texts:?}"
        );
    }

    #[tokio::test]
    async fn introspect_schema_sample_values_exclude_null() {
        let c = make_sales_connector();
        let info = c.introspect_schema().unwrap();

        let sales = info.tables.iter().find(|t| t.name == "sales").unwrap();
        let tag_col = sales.columns.iter().find(|c| c.name == "tag").unwrap();

        assert!(
            tag_col
                .sample_values
                .iter()
                .all(|v| !matches!(v, CellValue::Null)),
            "sample_values must not contain Null entries"
        );
    }

    #[tokio::test]
    async fn introspect_schema_detects_join_key_via_shared_id_column() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("orders.csv"),
            "order_id,customer_id,total\n1,10,50.0\n2,20,100.0\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("customers.csv"),
            "customer_id,region\n10,North\n20,South\n",
        )
        .unwrap();

        let c = DuckDbConnector::from_directory(dir.path(), LoadStrategy::View).unwrap();
        let info = c
            .introspect_schema()
            .expect("introspect_schema must succeed");

        let join = info
            .join_keys
            .iter()
            .find(|(_, _, col)| col == "customer_id");
        assert!(
            join.is_some(),
            "customer_id shared between tables must be detected as a join key"
        );
    }

    #[tokio::test]
    async fn introspect_schema_skips_system_tables() {
        let c = make_sales_connector();
        let info = c.introspect_schema().unwrap();

        for table in &info.tables {
            assert!(
                !table.name.starts_with("_agentic_"),
                "system table {} must be excluded",
                table.name
            );
            assert!(
                !table.name.to_lowercase().starts_with("information_schema"),
                "information_schema must not appear: {}",
                table.name
            );
        }
    }

    #[tokio::test]
    async fn introspect_schema_empty_db_returns_empty_schema_info() {
        let c = DuckDbConnector::in_memory().unwrap();
        let info = c.introspect_schema().expect("empty DB must not error");

        assert!(
            info.tables.is_empty(),
            "no tables in empty in-memory DuckDB"
        );
        assert!(
            info.join_keys.is_empty(),
            "no join keys in empty in-memory DuckDB"
        );
    }
}
