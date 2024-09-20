use arrow::record_batch::RecordBatch;
use arrow_cast::pretty::{pretty_format_batches, print_batches};
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use reqwest::Client;
use std::convert::TryFrom;
use std::error::Error;

use crate::ai::{generate_sql_query, interpret_results};
use crate::yaml_parsers::agent_parser::Agent;

async fn execute_bigquery_query(
    key_path: &str,
    query: &str,
) -> Result<Vec<RecordBatch>, Box<dyn Error>> {
    let conn_string = format!("bigquery://{}", key_path);
    let query = query.to_string(); // convert to owned string for closure

    let result = tokio::task::spawn_blocking(move || {
        let source_conn = SourceConn::try_from(conn_string.as_str())?;
        let queries = &[CXQuery::from(query.as_str())];
        let destination = get_arrow(&source_conn, None, queries).expect("Run failed at get_arrow.");
        destination.arrow()
    })
    .await??;

    Ok(result)
}

pub async fn process_input(
    agent: &Agent,
    client: &Client,
    api_key: &str,
    input: &str,
    model: &str,
    key_path: &str,
) -> Result<(), Box<dyn Error>> {
    let sql_query = generate_sql_query(client, api_key, input, model).await?;
    println!("Generated SQL query: {}", sql_query);

    let record_batches = execute_bigquery_query(key_path, &sql_query).await?;
    let result_string = pretty_format_batches(&record_batches)?;
    print_batches(&record_batches)?;

    let interpretation = interpret_results(
        client,
        api_key,
        input,
        &sql_query,
        &result_string.to_string(),
        model,
    )
    .await?;
    println!("Interpretation: {}", interpretation);

    Ok(())
}
