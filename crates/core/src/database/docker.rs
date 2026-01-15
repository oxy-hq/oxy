#![allow(deprecated)] // bollard container/image options are deprecated but still functional

use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ContainerStateStatusEnum, ContainerSummaryStateEnum, HostConfig,
    PortBinding,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, InspectContainerOptions, ListContainersOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use futures::StreamExt;
use oxy_shared::errors::OxyError;
use std::collections::HashMap;
use std::default::Default;
use tracing::{info, warn};

/// Docker PostgreSQL configuration constants
const POSTGRES_CONTAINER_NAME: &str = "oxy-postgres";
const POSTGRES_IMAGE: &str = "postgres:18-alpine";
const POSTGRES_PORT: u16 = 15432;
const POSTGRES_USERNAME: &str = "postgres";
const POSTGRES_PASSWORD: &str = "postgres";
const POSTGRES_DATABASE: &str = "oxy";
const POSTGRES_VOLUME: &str = "oxy-postgres-data";
pub const POSTGRES_READY_TIMEOUT_SECS: u64 = 30;

/// Get Docker client connection
async fn get_docker_client() -> Result<Docker, OxyError> {
    Docker::connect_with_local_defaults().map_err(|e| {
        OxyError::InitializationError(format!(
            "Failed to connect to Docker daemon. Please ensure Docker is installed and running. Error: {}",
            e
        ))
    })
}

/// Check if Docker is available on the system
pub async fn check_docker_available() -> Result<(), OxyError> {
    let docker = get_docker_client().await?;

    // Ping Docker daemon to verify it's responsive
    docker.ping().await.map_err(|e| {
        OxyError::InitializationError(format!(
            "Docker daemon is not responding. Please ensure Docker is running. Error: {}",
            e
        ))
    })?;

    Ok(())
}

/// Check if the PostgreSQL container is running
pub async fn is_postgres_container_running() -> Result<bool, OxyError> {
    let docker = get_docker_client().await?;

    let mut filters = HashMap::new();
    filters.insert(
        "name".to_string(),
        vec![POSTGRES_CONTAINER_NAME.to_string()],
    );

    let options = ListContainersOptions {
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to list containers: {}", e)))?;

    // Check if any container is running
    Ok(containers.iter().any(|c| {
        c.state
            .as_ref()
            .is_some_and(|s| *s == ContainerSummaryStateEnum::RUNNING)
    }))
}

/// Check if the PostgreSQL container exists (running or stopped)
async fn is_postgres_container_exists() -> Result<bool, OxyError> {
    let docker = get_docker_client().await?;

    let mut filters = HashMap::new();
    filters.insert(
        "name".to_string(),
        vec![POSTGRES_CONTAINER_NAME.to_string()],
    );

    let options = ListContainersOptions {
        all: true, // Include stopped containers
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to list containers: {}", e)))?;

    Ok(!containers.is_empty())
}

/// Start the PostgreSQL container and return the connection string
pub async fn start_postgres_container() -> Result<String, OxyError> {
    info!("Starting Docker PostgreSQL container...");
    let docker = get_docker_client().await?;

    // Check if container exists
    if is_postgres_container_exists().await? {
        info!("PostgreSQL container already exists");

        // Check if it's running
        if is_postgres_container_running().await? {
            info!("PostgreSQL container is already running");
        } else {
            info!("Starting existing PostgreSQL container...");
            docker
                .start_container(POSTGRES_CONTAINER_NAME, None::<StartContainerOptions>)
                .await
                .map_err(|e| {
                    OxyError::InitializationError(format!(
                        "Failed to start existing container: {}",
                        e
                    ))
                })?;
            info!("Existing PostgreSQL container started");
        }
    } else {
        info!("Creating new PostgreSQL container...");

        // Pull the image first (if not already present)
        info!("Pulling PostgreSQL image (if not already present)...");
        let create_image_options = CreateImageOptions {
            from_image: Some(POSTGRES_IMAGE.to_string()),
            ..Default::default()
        };

        let mut pull_stream = docker.create_image(Some(create_image_options), None, None);
        while let Some(result) = pull_stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status
                        && (status.contains("Downloaded") || status.contains("Pulling"))
                    {
                        tracing::debug!("{}", status);
                    }
                }
                Err(e) => {
                    warn!("Image pull warning: {}", e);
                }
            }
        }

        // Configure port bindings
        let mut port_bindings = HashMap::new();
        port_bindings.insert(
            "5432/tcp".to_string(),
            Some(vec![PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: Some(POSTGRES_PORT.to_string()),
            }]),
        );

        // Configure volume bindings
        let binds = vec![format!("{}:/var/lib/postgresql/data", POSTGRES_VOLUME)];

        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            ..Default::default()
        };

        // Configure environment variables
        let env: Vec<String> = vec![
            format!("POSTGRES_USER={}", POSTGRES_USERNAME),
            format!("POSTGRES_PASSWORD={}", POSTGRES_PASSWORD),
            format!("POSTGRES_DB={}", POSTGRES_DATABASE),
        ];

        // Create container configuration
        let config = ContainerCreateBody {
            image: Some(POSTGRES_IMAGE.to_string()),
            env: Some(env),
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: Some(POSTGRES_CONTAINER_NAME.to_string()),
            ..Default::default()
        };

        // Create and start the container
        docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| {
                OxyError::InitializationError(format!("Failed to create container: {}", e))
            })?;

        docker
            .start_container(POSTGRES_CONTAINER_NAME, None::<StartContainerOptions>)
            .await
            .map_err(|e| {
                OxyError::InitializationError(format!("Failed to start container: {}", e))
            })?;

        info!("PostgreSQL container created and started successfully");
    }

    // Generate connection string
    let connection_string = format!(
        "postgresql://{}:{}@localhost:{}/{}",
        POSTGRES_USERNAME, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_DATABASE
    );

    Ok(connection_string)
}

/// Wait for PostgreSQL to be ready to accept connections
pub async fn wait_for_postgres_ready(timeout_secs: u64) -> Result<(), OxyError> {
    info!(
        "Waiting for PostgreSQL to be ready (max {} seconds)...",
        timeout_secs
    );

    let docker = get_docker_client().await?;
    let start = std::time::Instant::now();
    let mut retry_count = 0;

    loop {
        // Check if timeout exceeded
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(OxyError::InitializationError(format!(
                "PostgreSQL did not become ready within {} seconds",
                timeout_secs
            )));
        }

        // Check container status first
        let inspect_result = docker
            .inspect_container(POSTGRES_CONTAINER_NAME, None::<InspectContainerOptions>)
            .await;

        match inspect_result {
            Ok(container) => {
                // Check if container is running
                if let Some(state) = container.state
                    && state.status != Some(ContainerStateStatusEnum::RUNNING)
                {
                    return Err(OxyError::InitializationError(format!(
                        "PostgreSQL container is not running (status: {:?})",
                        state.status
                    )));
                }

                // Try pg_isready command
                let exec_config = CreateExecOptions {
                    cmd: Some(vec![
                        "pg_isready",
                        "-U",
                        POSTGRES_USERNAME,
                        "-d",
                        POSTGRES_DATABASE,
                    ]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                };

                let exec_result = docker
                    .create_exec(POSTGRES_CONTAINER_NAME, exec_config)
                    .await;

                if let Ok(exec_response) = exec_result
                    && let Ok(StartExecResults::Attached { mut output, .. }) =
                        docker.start_exec(&exec_response.id, None).await
                {
                    // Read output to check exit code
                    let mut stdout = Vec::new();
                    while let Some(Ok(msg)) = output.next().await {
                        stdout.extend_from_slice(&msg.into_bytes());
                    }

                    // If we got here, pg_isready executed successfully
                    if String::from_utf8_lossy(&stdout).contains("accepting connections") {
                        info!("PostgreSQL is ready!");
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                return Err(OxyError::InitializationError(format!(
                    "Failed to inspect container: {}",
                    e
                )));
            }
        }

        retry_count += 1;
        if retry_count == 1 || retry_count % 5 == 0 {
            info!(
                "PostgreSQL not ready yet, retrying... ({} seconds elapsed)",
                start.elapsed().as_secs()
            );
        }

        // Wait before retrying (exponential backoff with cap at 2 seconds)
        let wait_ms = std::cmp::min(100 * 2u64.pow(retry_count.min(4)), 2000);
        tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
    }
}

/// Stop the PostgreSQL container
pub async fn stop_postgres_container() -> Result<(), OxyError> {
    if !is_postgres_container_running().await? {
        info!("PostgreSQL container is not running, nothing to stop");
        return Ok(());
    }

    info!("Stopping PostgreSQL container...");
    let docker = get_docker_client().await?;

    // Stop the container with a grace period of 10 seconds
    let options = StopContainerOptions {
        t: Some(10),
        signal: None,
    };

    docker
        .stop_container(POSTGRES_CONTAINER_NAME, Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to stop container: {}", e)))?;

    info!("PostgreSQL container stopped successfully");
    Ok(())
}

/// Remove the PostgreSQL container (useful for cleanup or troubleshooting)
#[allow(dead_code)]
pub async fn remove_postgres_container() -> Result<(), OxyError> {
    if !is_postgres_container_exists().await? {
        info!("PostgreSQL container does not exist, nothing to remove");
        return Ok(());
    }

    // Stop first if running
    if is_postgres_container_running().await? {
        stop_postgres_container().await?;
    }

    info!("Removing PostgreSQL container...");
    let docker = get_docker_client().await?;

    let options = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };

    docker
        .remove_container(POSTGRES_CONTAINER_NAME, Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to remove container: {}", e)))?;

    info!("PostgreSQL container removed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_string_format() {
        let expected = format!(
            "postgresql://{}:{}@localhost:{}/{}",
            POSTGRES_USERNAME, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_DATABASE
        );
        assert_eq!(
            expected,
            "postgresql://postgres:postgres@localhost:15432/oxy"
        );
    }

    #[test]
    fn test_constants() {
        assert_eq!(POSTGRES_CONTAINER_NAME, "oxy-postgres");
        assert_eq!(POSTGRES_IMAGE, "postgres:18-alpine");
        assert_eq!(POSTGRES_PORT, 15432);
        assert_eq!(POSTGRES_USERNAME, "postgres");
        assert_eq!(POSTGRES_DATABASE, "oxy");
    }
}
