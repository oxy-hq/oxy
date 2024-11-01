use onyx::cli::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli().await?;
    Ok(())
}