use oxy::database::docker;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use std::process::Command;

/// Display status of Oxy services and Docker containers
pub async fn show_status() -> Result<(), OxyError> {
    println!("{}", "=== Oxy Service Status ===\n".text());

    // 1. Check Docker daemon status
    print_docker_daemon_status();
    println!();

    // 2. Check PostgreSQL container status
    check_postgres_container_status().await;
    println!();

    // 3. Check database connectivity
    check_database_connectivity().await;
    println!();

    // 4. Show helpful commands
    print_helpful_commands();

    Ok(())
}

fn print_docker_daemon_status() {
    println!("{}", "Docker Daemon:".text());

    match Command::new("docker").args(["info"]).output() {
        Ok(output) if output.status.success() => {
            println!("  Status: {}", "Running".success());

            // Extract useful info from docker info
            let info = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = info.lines().find(|l| l.contains("Server Version:")) {
                println!("  {}", line.trim());
            }
        }
        Ok(_) => {
            println!("  Status: {}", "Not Running".error());
            println!(
                "  {}",
                "→ Start Docker to use Oxy with local PostgreSQL".tertiary()
            );
        }
        Err(_) => {
            println!("  Status: {}", "Not Installed".error());
            println!(
                "  {}",
                "→ Install Docker from https://www.docker.com/".tertiary()
            );
        }
    }
}

async fn check_postgres_container_status() {
    println!("{}", "PostgreSQL Container:".text());

    // Check if container exists
    let exists_output = Command::new("docker")
        .args(["ps", "-a", "-q", "-f", "name=oxy-postgres"])
        .output();

    match exists_output {
        Ok(output) if !output.stdout.is_empty() => {
            // Container exists, check if it's running
            match docker::is_postgres_container_running().await {
                Ok(true) => {
                    println!("  Status: {}", "Running".success());
                    println!("  Container: oxy-postgres");
                    println!("  Port: 15432:5432");
                    println!("  Volume: oxy-postgres-data");

                    // Get container uptime
                    if let Ok(inspect) = Command::new("docker")
                        .args(["inspect", "-f", "{{.State.StartedAt}}", "oxy-postgres"])
                        .output()
                    {
                        let started_at = String::from_utf8_lossy(&inspect.stdout);
                        println!("  Started: {}", started_at.trim());
                    }
                }
                Ok(false) => {
                    println!("  Status: {}", "Stopped".warning());
                    println!("  Container: oxy-postgres");
                    println!(
                        "  {}",
                        "→ Run 'oxy start' to start the container".tertiary()
                    );
                }
                Err(_) => {
                    println!("  Status: {}", "Unknown".warning());
                }
            }
        }
        Ok(_) => {
            println!("  Status: {}", "Not Created".warning());
            println!(
                "  {}",
                "→ Run 'oxy start' to create and start the container".tertiary()
            );
        }
        Err(_) => {
            println!("  Status: {}", "Unable to check".error());
            println!("  {}", "→ Docker may not be running".tertiary());
        }
    }
}

async fn check_database_connectivity() {
    println!("{}", "Database Connection:".text());

    // Check if using external database
    if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
        println!("  Mode: {}", "External PostgreSQL".text());
        println!("  URL: {}", mask_password(&url));

        // Try to connect
        match sea_orm::Database::connect(&url).await {
            Ok(_) => {
                println!("  Status: {}", "Connected".success());
            }
            Err(e) => {
                println!("  Status: {}", "Connection Failed".error());
                println!("  Error: {}", e.to_string().error());
            }
        }
    } else {
        // Using Docker PostgreSQL
        println!("  Mode: {}", "Docker PostgreSQL".text());

        match docker::is_postgres_container_running().await {
            Ok(true) => {
                // Try to connect
                let conn_str = "postgresql://postgres:postgres@localhost:15432/oxy";
                match sea_orm::Database::connect(conn_str).await {
                    Ok(_) => {
                        println!("  Status: {}", "Connected".success());
                        println!("  URL: postgresql://postgres:***@localhost:15432/oxy");
                    }
                    Err(e) => {
                        println!("  Status: {}", "Connection Failed".error());
                        println!("  Error: {}", e.to_string().error());
                        println!("  {}", "→ Container may still be starting up".tertiary());
                    }
                }
            }
            Ok(false) => {
                println!("  Status: {}", "Not Running".warning());
                println!("  {}", "→ Run 'oxy start' to start PostgreSQL".tertiary());
            }
            Err(_) => {
                println!("  Status: {}", "Unknown".error());
            }
        }
    }
}

fn print_helpful_commands() {
    println!("{}", "Useful Commands:".text());
    println!(
        "  View PostgreSQL logs:  {}",
        "docker logs oxy-postgres".secondary()
    );
    println!(
        "  Follow logs:           {}",
        "docker logs -f oxy-postgres".secondary()
    );
    println!(
        "  Access PostgreSQL:     {}",
        "docker exec -it oxy-postgres psql -U postgres -d oxy".secondary()
    );
    println!(
        "  Stop PostgreSQL:       {}",
        "docker stop oxy-postgres".secondary()
    );
    println!(
        "  Remove container:      {}",
        "docker rm oxy-postgres".secondary()
    );
    println!(
        "  Remove volume:         {}",
        "docker volume rm oxy-postgres-data".secondary()
    );
}

/// Mask password in connection string for display
fn mask_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@')
        && let Some(protocol_end) = url.find("://")
    {
        let protocol = &url[..protocol_end + 3];
        let after_at = &url[at_pos..];

        // Find the colon after username
        let middle = &url[protocol_end + 3..at_pos];
        if let Some(colon_pos) = middle.find(':') {
            let username = &middle[..colon_pos];
            return format!("{}{}:***{}", protocol, username, after_at);
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_password() {
        assert_eq!(
            mask_password("postgresql://user:password@localhost:5432/db"),
            "postgresql://user:***@localhost:5432/db"
        );
        assert_eq!(
            mask_password("postgresql://postgres:secret123@db.example.com:5432/mydb"),
            "postgresql://postgres:***@db.example.com:5432/mydb"
        );
    }
}
