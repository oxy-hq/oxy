use onyx::cli::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    cli().await?;
    Ok(())
}
