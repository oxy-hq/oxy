use sea_orm_migration::prelude::*;

#[async_std::main]
async fn main() {
    // sea_orm_migration's CLI hardcodes DATABASE_URL. The rest of oxy uses
    // OXY_DATABASE_URL; mirror it here so `cargo run -p migration` shares
    // the same env file.
    if std::env::var_os("DATABASE_URL").is_none()
        && let Ok(url) = std::env::var("OXY_DATABASE_URL")
    {
        // SAFETY: single-threaded; pre-main.
        unsafe {
            std::env::set_var("DATABASE_URL", url);
        }
    }
    cli::run_cli(migration::Migrator).await;
}
