#![allow(deprecated)] // bollard container/image options are deprecated but still functional

use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ContainerStateStatusEnum, ContainerSummaryStateEnum, HostConfig,
    PortBinding,
};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, InspectContainerOptions, ListContainersOptions,
    RemoveContainerOptions, RemoveVolumeOptions, StartContainerOptions, StopContainerOptions,
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

/// Docker ClickHouse configuration constants
const CLICKHOUSE_CONTAINER_NAME: &str = "oxy-clickhouse";
const CLICKHOUSE_IMAGE: &str = "clickhouse/clickhouse-server:latest";
const CLICKHOUSE_HTTP_PORT: u16 = 8123;
const CLICKHOUSE_NATIVE_PORT: u16 = 9000;
const CLICKHOUSE_USER: &str = "default";
const CLICKHOUSE_PASSWORD: &str = "default";
const CLICKHOUSE_DATABASE: &str = "otel";
const CLICKHOUSE_VOLUME: &str = "oxy-clickhouse-data";
pub const CLICKHOUSE_READY_TIMEOUT_SECS: u64 = 30;

/// Docker OTel Collector configuration constants
const OTEL_CONTAINER_NAME: &str = "oxy-otel-collector";
const OTEL_IMAGE: &str = "otel/opentelemetry-collector-contrib:0.144.0";
const OTEL_GRPC_PORT: u16 = 4317;
const OTEL_HTTP_PORT: u16 = 4318;

/// Docker Cube.js configuration constants
const CUBEJS_CONTAINER_NAME: &str = "oxy-cubejs";
const CUBEJS_IMAGE: &str = "cubejs/cube:v1.3.81";
const CUBEJS_PORT: u16 = 4000;
pub const CUBEJS_READY_TIMEOUT_SECS: u64 = 30;

/// Docker network for enterprise services
const ENTERPRISE_NETWORK: &str = "oxy-enterprise";

/// Embedded OTel Collector configuration (fallback if file not found in cwd)
const OTEL_COLLECTOR_CONFIG: &str = include_str!("../../../../otel-collector-config.yaml");

/// OTel Collector config file name
const OTEL_CONFIG_FILENAME: &str = "otel-collector-config.yaml";

/// Get Docker client connection
///
/// This function attempts to connect to a Docker-compatible container runtime.
/// It works with Docker Desktop, Rancher Desktop (moby mode), Colima, Podman (with Docker socket),
/// and other Docker API-compatible runtimes.
///
/// Connection is determined by the DOCKER_HOST environment variable (if set) or system defaults.
/// Supported DOCKER_HOST formats:
/// - `unix:///path/to/docker.sock` - Unix socket (Linux/macOS)
/// - `npipe:////./pipe/docker_engine` - Named pipe (Windows)
/// - `tcp://host:port` - TCP connection
async fn get_docker_client() -> Result<Docker, OxyError> {
    // Try default connection first (respects DOCKER_HOST env var)
    if let Ok(docker) = Docker::connect_with_local_defaults() {
        return Ok(docker);
    }

    // Fallback: try common alternative socket paths (especially for macOS)
    // These paths are used when Docker Desktop or other runtimes are installed without admin
    #[cfg(unix)]
    {
        let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let alternative_paths = vec![
            // Docker Desktop (user installation on macOS)
            format!("{}/.docker/run/docker.sock", home_dir),
            // Colima
            format!("{}/.colima/default/docker.sock", home_dir),
            // Rancher Desktop
            format!("{}/.rd/docker.sock", home_dir),
            // Podman (macOS machine)
            format!(
                "{}/.local/share/containers/podman/machine/podman.sock",
                home_dir
            ),
            // OrbStack
            format!("{}/.orbstack/run/docker.sock", home_dir),
            // Standard system paths
            "/var/run/docker.sock".to_string(),
            "/run/docker.sock".to_string(),
        ];

        for socket_path in alternative_paths {
            // Check if socket file exists before attempting connection
            if std::path::Path::new(&socket_path).exists() {
                if let Ok(docker) =
                    Docker::connect_with_unix(&socket_path, 120, bollard::API_DEFAULT_VERSION)
                {
                    tracing::debug!("Connected to Docker using socket: {}", socket_path);
                    return Ok(docker);
                }
            }
        }
    }

    Err(OxyError::InitializationError(
        "Failed to connect to container runtime.\n\n\
         💡 Troubleshooting:\n\
         • Ensure a Docker-compatible container runtime is installed and running\n\
         • Supported: Docker Desktop, Rancher Desktop, Colima, Podman, OrbStack, etc.\n\
         • Verify with: docker ps\n\
         • Set DOCKER_HOST environment variable for custom socket location\n\
         • On macOS, common socket paths:\n\
           - Docker Desktop (admin): /var/run/docker.sock\n\
           - Docker Desktop (user): ~/.docker/run/docker.sock\n\
           - Colima: ~/.colima/default/docker.sock\n\
           - Rancher Desktop: ~/.rd/docker.sock\n\
           - OrbStack: ~/.orbstack/run/docker.sock\n\n\
         📚 See https://docs.oxy.tech/deployment/container-runtimes for setup instructions"
            .to_string(),
    ))
}

/// Check if Docker is available on the system
pub async fn check_docker_available() -> Result<(), OxyError> {
    let docker = get_docker_client().await?;

    // Ping Docker daemon to verify it's responsive
    docker.ping().await.map_err(|e| {
        OxyError::InitializationError(format!(
            "Container runtime is not responding.\n\
             Error: {}\n\n\
             💡 Common solutions:\n\
             • Start your container runtime (Docker Desktop, Rancher Desktop, Colima, etc.)\n\
             • Wait 30-60 seconds for the runtime to fully initialize\n\
             • Check runtime status: docker ps\n\
             • For Rancher Desktop: ensure 'dockerd (moby)' is selected, not 'containerd'\n\
             • For Colima: verify with: colima status\n\
             • For Podman: ensure Docker socket is enabled",
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

// === Enterprise Docker Services (ClickHouse + OTel Collector) ===

/// Ensure the enterprise Docker network exists
async fn ensure_enterprise_network() -> Result<(), OxyError> {
    let docker = get_docker_client().await?;

    // Check if network already exists
    let networks = docker
        .list_networks(None::<bollard::query_parameters::ListNetworksOptions>)
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to list networks: {}", e)))?;

    let exists = networks
        .iter()
        .any(|n| n.name.as_deref() == Some(ENTERPRISE_NETWORK));

    if !exists {
        info!("Creating Docker network '{}'...", ENTERPRISE_NETWORK);
        let config = bollard::models::NetworkCreateRequest {
            name: ENTERPRISE_NETWORK.to_string(),
            driver: Some("bridge".to_string()),
            ..Default::default()
        };
        docker.create_network(config).await.map_err(|e| {
            OxyError::InitializationError(format!("Failed to create network: {}", e))
        })?;
    }

    Ok(())
}

/// Check if a container exists (running or stopped)
async fn is_container_exists(container_name: &str) -> Result<bool, OxyError> {
    let docker = get_docker_client().await?;

    let mut filters = HashMap::new();
    filters.insert("name".to_string(), vec![container_name.to_string()]);

    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to list containers: {}", e)))?;

    Ok(!containers.is_empty())
}

/// Check if a container is running
async fn is_container_running(container_name: &str) -> Result<bool, OxyError> {
    let docker = get_docker_client().await?;

    let mut filters = HashMap::new();
    filters.insert("name".to_string(), vec![container_name.to_string()]);

    let options = ListContainersOptions {
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .map_err(|e| OxyError::InitializationError(format!("Failed to list containers: {}", e)))?;

    Ok(containers.iter().any(|c| {
        c.state
            .as_ref()
            .is_some_and(|s| *s == ContainerSummaryStateEnum::RUNNING)
    }))
}

/// Start the ClickHouse container
pub async fn start_clickhouse_container() -> Result<(), OxyError> {
    info!("Starting Docker ClickHouse container...");
    let docker = get_docker_client().await?;

    ensure_enterprise_network().await?;

    if is_container_exists(CLICKHOUSE_CONTAINER_NAME).await? {
        if is_container_running(CLICKHOUSE_CONTAINER_NAME).await? {
            info!("ClickHouse container is already running");
            return Ok(());
        }
        info!("Starting existing ClickHouse container...");
        docker
            .start_container(CLICKHOUSE_CONTAINER_NAME, None::<StartContainerOptions>)
            .await
            .map_err(|e| {
                OxyError::InitializationError(format!(
                    "Failed to start existing ClickHouse container: {}",
                    e
                ))
            })?;
        return Ok(());
    }

    // Pull the image
    info!("Pulling ClickHouse image...");
    let create_image_options = CreateImageOptions {
        from_image: Some(CLICKHOUSE_IMAGE.to_string()),
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
        "8123/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(CLICKHOUSE_HTTP_PORT.to_string()),
        }]),
    );
    port_bindings.insert(
        "9000/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(CLICKHOUSE_NATIVE_PORT.to_string()),
        }]),
    );

    let binds = vec![format!("{}:/var/lib/clickhouse", CLICKHOUSE_VOLUME)];

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        network_mode: Some(ENTERPRISE_NETWORK.to_string()),
        ..Default::default()
    };

    let env: Vec<String> = vec![
        format!("CLICKHOUSE_DB={}", CLICKHOUSE_DATABASE),
        format!("CLICKHOUSE_USER={}", CLICKHOUSE_USER),
        format!("CLICKHOUSE_PASSWORD={}", CLICKHOUSE_PASSWORD),
    ];

    let config = ContainerCreateBody {
        image: Some(CLICKHOUSE_IMAGE.to_string()),
        env: Some(env),
        hostname: Some("clickhouse".to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(CLICKHOUSE_CONTAINER_NAME.to_string()),
        ..Default::default()
    };

    docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to create ClickHouse container: {}", e))
        })?;

    docker
        .start_container(CLICKHOUSE_CONTAINER_NAME, None::<StartContainerOptions>)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to start ClickHouse container: {}", e))
        })?;

    info!("ClickHouse container created and started successfully");
    Ok(())
}

/// Wait for ClickHouse to be ready to accept connections
pub async fn wait_for_clickhouse_ready(timeout_secs: u64) -> Result<(), OxyError> {
    info!(
        "Waiting for ClickHouse to be ready (max {} seconds)...",
        timeout_secs
    );

    let docker = get_docker_client().await?;
    let start = std::time::Instant::now();
    let mut retry_count = 0u32;

    loop {
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(OxyError::InitializationError(format!(
                "ClickHouse did not become ready within {} seconds",
                timeout_secs
            )));
        }

        let exec_config = CreateExecOptions {
            cmd: Some(vec!["clickhouse-client", "--query", "SELECT 1"]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec_result = docker
            .create_exec(CLICKHOUSE_CONTAINER_NAME, exec_config)
            .await;

        if let Ok(exec_response) = exec_result
            && let Ok(StartExecResults::Attached { mut output, .. }) =
                docker.start_exec(&exec_response.id, None).await
        {
            let mut stdout = Vec::new();
            while let Some(Ok(msg)) = output.next().await {
                stdout.extend_from_slice(&msg.into_bytes());
            }

            if String::from_utf8_lossy(&stdout).trim() == "1" {
                info!("ClickHouse is ready!");
                return Ok(());
            }
        }

        retry_count += 1;
        if retry_count == 1 || retry_count.is_multiple_of(5) {
            info!(
                "ClickHouse not ready yet, retrying... ({} seconds elapsed)",
                start.elapsed().as_secs()
            );
        }

        let wait_ms = std::cmp::min(100 * 2u64.pow(retry_count.min(4)), 2000);
        tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
    }
}

/// Start the OpenTelemetry Collector container
pub async fn start_otel_collector_container() -> Result<(), OxyError> {
    info!("Starting Docker OTel Collector container...");
    let docker = get_docker_client().await?;

    ensure_enterprise_network().await?;

    if is_container_exists(OTEL_CONTAINER_NAME).await? {
        if is_container_running(OTEL_CONTAINER_NAME).await? {
            info!("OTel Collector container is already running");
            return Ok(());
        }
        info!("Starting existing OTel Collector container...");
        docker
            .start_container(OTEL_CONTAINER_NAME, None::<StartContainerOptions>)
            .await
            .map_err(|e| {
                OxyError::InitializationError(format!(
                    "Failed to start existing OTel Collector container: {}",
                    e
                ))
            })?;
        return Ok(());
    }

    // Pull the image
    info!("Pulling OTel Collector image...");
    let create_image_options = CreateImageOptions {
        from_image: Some(OTEL_IMAGE.to_string()),
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

    // Use otel-collector-config.yaml from the working directory if available,
    // otherwise fall back to the embedded default written to a temp file.
    let local_config = std::env::current_dir()
        .ok()
        .map(|d| d.join(OTEL_CONFIG_FILENAME))
        .filter(|p| p.is_file()); // Only accept files, not directories

    let config_path = if let Some(path) = local_config {
        info!("Using OTel config from: {}", path.display());
        path
    } else {
        // Use a unique temp file name to avoid conflicts
        let pid = std::process::id();
        let tmp = std::env::temp_dir().join(format!("oxy-otel-collector-config-{}.yaml", pid));

        // If path exists as a directory, remove it first
        if tmp.exists() && tmp.is_dir() {
            std::fs::remove_dir_all(&tmp).ok();
        }

        std::fs::write(&tmp, OTEL_COLLECTOR_CONFIG).map_err(|e| {
            OxyError::InitializationError(format!("Failed to write OTel config file: {}", e))
        })?;
        info!("Using embedded OTel config (written to {})", tmp.display());
        tmp
    };

    // Configure port bindings
    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        "4317/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(OTEL_GRPC_PORT.to_string()),
        }]),
    );
    port_bindings.insert(
        "4318/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(OTEL_HTTP_PORT.to_string()),
        }]),
    );

    let binds = vec![format!(
        "{}:/etc/otel-collector-config.yaml",
        config_path.display()
    )];

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        network_mode: Some(ENTERPRISE_NETWORK.to_string()),
        ..Default::default()
    };

    let config = ContainerCreateBody {
        image: Some(OTEL_IMAGE.to_string()),
        cmd: Some(vec!["--config=/etc/otel-collector-config.yaml".to_string()]),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(OTEL_CONTAINER_NAME.to_string()),
        ..Default::default()
    };

    docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!(
                "Failed to create OTel Collector container: {}",
                e
            ))
        })?;

    docker
        .start_container(OTEL_CONTAINER_NAME, None::<StartContainerOptions>)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!(
                "Failed to start OTel Collector container: {}",
                e
            ))
        })?;

    info!("OTel Collector container created and started successfully");
    Ok(())
}

/// Stop the ClickHouse container
pub async fn stop_clickhouse_container() -> Result<(), OxyError> {
    if !is_container_running(CLICKHOUSE_CONTAINER_NAME).await? {
        info!("ClickHouse container is not running, nothing to stop");
        return Ok(());
    }

    info!("Stopping ClickHouse container...");
    let docker = get_docker_client().await?;

    let options = StopContainerOptions {
        t: Some(10),
        signal: None,
    };

    docker
        .stop_container(CLICKHOUSE_CONTAINER_NAME, Some(options))
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to stop ClickHouse container: {}", e))
        })?;

    info!("ClickHouse container stopped successfully");
    Ok(())
}

/// Stop the OTel Collector container
pub async fn stop_otel_collector_container() -> Result<(), OxyError> {
    if !is_container_running(OTEL_CONTAINER_NAME).await? {
        info!("OTel Collector container is not running, nothing to stop");
        return Ok(());
    }

    info!("Stopping OTel Collector container...");
    let docker = get_docker_client().await?;

    let options = StopContainerOptions {
        t: Some(5),
        signal: None,
    };

    docker
        .stop_container(OTEL_CONTAINER_NAME, Some(options))
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to stop OTel Collector container: {}", e))
        })?;

    info!("OTel Collector container stopped successfully");
    Ok(())
}

/// Start the Cube.js semantic engine container
pub async fn start_cubejs_container(
    cube_config_dir: String,
    project_path: String,
    db_url: String,
    dev_mode: bool,
    log_level: String,
) -> Result<(), OxyError> {
    info!("Starting Docker Cube.js container...");
    let docker = get_docker_client().await?;

    ensure_enterprise_network().await?;

    if is_container_exists(CUBEJS_CONTAINER_NAME).await? {
        if is_container_running(CUBEJS_CONTAINER_NAME).await? {
            info!("Cube.js container is already running");
            return Ok(());
        }
        info!("Starting existing Cube.js container...");
        docker
            .start_container(CUBEJS_CONTAINER_NAME, None::<StartContainerOptions>)
            .await
            .map_err(|e| {
                OxyError::InitializationError(format!(
                    "Failed to start existing Cube.js container: {}",
                    e
                ))
            })?;
        return Ok(());
    }

    // Pull the image
    info!("Pulling Cube.js image...");
    let create_image_options = CreateImageOptions {
        from_image: Some(CUBEJS_IMAGE.to_string()),
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
        "4000/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(CUBEJS_PORT.to_string()),
        }]),
    );

    // Configure volume bindings
    let binds = vec![
        format!("{}:/cube/conf", cube_config_dir),
        format!("{}/.db:/cube/.db", project_path),
    ];

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(binds),
        network_mode: Some(ENTERPRISE_NETWORK.to_string()),
        ..Default::default()
    };

    // Configure environment variables
    let env: Vec<String> = vec![
        format!("CUBEJS_DEV_MODE={}", dev_mode),
        format!("CUBEJS_LOG_LEVEL={}", log_level),
        format!("CUBEJS_DB_URL={}", db_url),
    ];

    let config = ContainerCreateBody {
        image: Some(CUBEJS_IMAGE.to_string()),
        env: Some(env),
        hostname: Some("cubejs".to_string()),
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(CUBEJS_CONTAINER_NAME.to_string()),
        ..Default::default()
    };

    docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to create Cube.js container: {}", e))
        })?;

    docker
        .start_container(CUBEJS_CONTAINER_NAME, None::<StartContainerOptions>)
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to start Cube.js container: {}", e))
        })?;

    info!("Cube.js container created and started successfully");
    Ok(())
}

/// Wait for Cube.js to be ready to accept connections
pub async fn wait_for_cubejs_ready(timeout_secs: u64) -> Result<(), OxyError> {
    info!(
        "Waiting for Cube.js to be ready (max {} seconds)...",
        timeout_secs
    );

    let docker = get_docker_client().await?;
    let start = std::time::Instant::now();
    let mut retry_count = 0u32;

    loop {
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(OxyError::InitializationError(format!(
                "Cube.js did not become ready within {} seconds",
                timeout_secs
            )));
        }

        // Check if container is running
        let inspect_result = docker
            .inspect_container(CUBEJS_CONTAINER_NAME, None::<InspectContainerOptions>)
            .await;

        match inspect_result {
            Ok(container) => {
                if let Some(state) = &container.state
                    && state.status == Some(ContainerStateStatusEnum::RUNNING)
                {
                    // For Cube.js, we'll check if the HTTP endpoint is responsive
                    // This is a simple check - in production you might want to check /readyz or /livez
                    if let Ok(response) = reqwest::get("http://localhost:4000/").await
                        && (response.status().is_success() || response.status().is_client_error())
                    {
                        // 200 or 4xx means the server is up (4xx is expected for unauthenticated requests)
                        info!("Cube.js is ready!");
                        return Ok(());
                    }
                } else {
                    return Err(OxyError::InitializationError(format!(
                        "Cube.js container is not running (status: {:?})",
                        container.state.as_ref().and_then(|s| s.status.as_ref())
                    )));
                }
            }
            Err(e) => {
                return Err(OxyError::InitializationError(format!(
                    "Failed to inspect Cube.js container: {}",
                    e
                )));
            }
        }

        retry_count += 1;
        if retry_count == 1 || retry_count.is_multiple_of(5) {
            info!(
                "Cube.js not ready yet, retrying... ({} seconds elapsed)",
                start.elapsed().as_secs()
            );
        }

        let wait_ms = std::cmp::min(100 * 2u64.pow(retry_count.min(4)), 2000);
        tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
    }
}

/// Stop the Cube.js container
pub async fn stop_cubejs_container() -> Result<(), OxyError> {
    if !is_container_running(CUBEJS_CONTAINER_NAME).await? {
        info!("Cube.js container is not running, nothing to stop");
        return Ok(());
    }

    info!("Stopping Cube.js container...");
    let docker = get_docker_client().await?;

    let options = StopContainerOptions {
        t: Some(5),
        signal: None,
    };

    docker
        .stop_container(CUBEJS_CONTAINER_NAME, Some(options))
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to stop Cube.js container: {}", e))
        })?;

    info!("Cube.js container stopped successfully");
    Ok(())
}

/// Stop all enterprise containers (ClickHouse + OTel Collector + Cube.js)
pub async fn stop_enterprise_containers() -> Result<(), OxyError> {
    // Stop OTel first since it depends on ClickHouse
    if let Err(e) = stop_otel_collector_container().await {
        warn!("Failed to stop OTel Collector container: {}", e);
    }
    if let Err(e) = stop_cubejs_container().await {
        warn!("Failed to stop Cube.js container: {}", e);
    }
    if let Err(e) = stop_clickhouse_container().await {
        warn!("Failed to stop ClickHouse container: {}", e);
    }
    Ok(())
}

/// Remove a container by name (stop first if running, then remove)
async fn remove_container(docker: &Docker, name: &str) -> Result<(), OxyError> {
    if !is_container_exists(name).await? {
        return Ok(());
    }

    // Stop if running
    if is_container_running(name).await? {
        let stop_opts = StopContainerOptions {
            t: Some(5),
            signal: None,
        };
        if let Err(e) = docker.stop_container(name, Some(stop_opts)).await {
            warn!("Failed to stop container '{}': {}", name, e);
        }
    }

    // Remove
    let remove_opts = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    docker
        .remove_container(name, Some(remove_opts))
        .await
        .map_err(|e| {
            OxyError::InitializationError(format!("Failed to remove container '{}': {}", name, e))
        })?;

    info!("Removed container '{}'", name);
    Ok(())
}

/// Remove a Docker volume by name
async fn remove_volume(docker: &Docker, name: &str) -> Result<(), OxyError> {
    let opts = RemoveVolumeOptions { force: true };
    match docker.remove_volume(name, Some(opts)).await {
        Ok(_) => {
            info!("Removed volume '{}'", name);
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => {
            // Volume doesn't exist, that's fine
            Ok(())
        }
        Err(e) => Err(OxyError::InitializationError(format!(
            "Failed to remove volume '{}': {}",
            name, e
        ))),
    }
}

/// Remove the enterprise Docker network
async fn remove_enterprise_network(docker: &Docker) -> Result<(), OxyError> {
    match docker.remove_network(ENTERPRISE_NETWORK).await {
        Ok(_) => {
            info!("Removed network '{}'", ENTERPRISE_NETWORK);
            Ok(())
        }
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(e) => Err(OxyError::InitializationError(format!(
            "Failed to remove network '{}': {}",
            ENTERPRISE_NETWORK, e
        ))),
    }
}

/// Clean all Oxy-managed Docker containers, volumes, and networks.
/// Used by `oxy start --clean` to start from a fresh state.
pub async fn clean_all(enterprise: bool) -> Result<(), OxyError> {
    let docker = get_docker_client().await?;

    if enterprise {
        // Remove enterprise containers (order: otel → cubejs → clickhouse)
        remove_container(&docker, OTEL_CONTAINER_NAME).await?;
        remove_container(&docker, CUBEJS_CONTAINER_NAME).await?;
        remove_container(&docker, CLICKHOUSE_CONTAINER_NAME).await?;
    }

    // Remove postgres container
    remove_container(&docker, POSTGRES_CONTAINER_NAME).await?;

    if enterprise {
        // Remove enterprise volumes
        remove_volume(&docker, CLICKHOUSE_VOLUME).await?;
        // Remove enterprise network
        remove_enterprise_network(&docker).await?;
    }

    // Remove postgres volume
    remove_volume(&docker, POSTGRES_VOLUME).await?;

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
