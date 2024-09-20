use reqwest::Client;
use serde_json::json;
use std::error::Error;

pub async fn generate_ai_response(
    client: &Client,
    api_key: &str,
    system_message: &str,
    user_input: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    let request = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": system_message
            },
            {
                "role": "user",
                "content": user_input
            }
        ]
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(response["choices"][0]["message"]["content"]
        .as_str()
        .expect("Failed to get content from OpenAI")
        .to_string())
}

pub async fn interpret_results(
    client: &Client,
    api_key: &str,
    input: &str,
    sql_query: &str,
    result_string: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    let system_message = "You are a data analyst. Interpret the following query results and provide a concise summary.";
    let user_message = format!(
        "Question: {}\n\nSQL Query: {}\n\nQuery Results: {}",
        input, sql_query, result_string
    );

    generate_ai_response(client, api_key, system_message, &user_message, model).await
}

pub async fn generate_sql_query(
    client: &Client,
    api_key: &str,
    input: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    let system_message = "You are an SQL expert. Your task is to generate SQL queries based on user requests. Provide only the SQL query without any explanation or additional text.";

    let user_message = format!("Generate a SQL query for the following request: {}", input);

    let sql_query =
        generate_ai_response(client, api_key, system_message, &user_message, model).await?;

    // Basic validation to ensure the response looks like a SQL query
    if !sql_query.trim().to_lowercase().starts_with("select") {
        return Err("Generated response does not appear to be a valid SQL query".into());
    }

    Ok(sql_query)
}
