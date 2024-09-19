use reqwest::Client;
use serde_json::json;
use std::error::Error;

pub async fn generate_ai_response(client: &Client, api_key: &str, system_message: &str, user_input: &str, model: &str) -> Result<String, Box<dyn Error>> {
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
    result_json: &str,
    model: &str
) -> Result<String, Box<dyn Error>> {
    let interpret_request = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a data analyst. Interpret the following query results and provide a concise summary."
            },
            {
                "role": "user",
                "content": format!("Question: {}\n\nSQL Query: {}\n\nQuery Results: {}", input, sql_query, result_json)
            }
        ]
    });

    let interpret_response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&interpret_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(interpret_response["choices"][0]["message"]["content"]
        .as_str()
        .expect("Failed to get interpretation from OpenAI")
        .to_string())
}
