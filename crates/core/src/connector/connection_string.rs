//! Connection string parsing and formatting utilities.
//!
//! This module provides traits and implementations for parsing database connection
//! strings from URL format and formatting them for different output dialects.
//!
//! # Extensibility
//!
//! To add support for a new database dialect:
//! 1. Create a new struct representing the parsed connection (e.g., `MySqlConnectionString`)
//! 2. Implement `ConnectionStringParser` trait for parsing URL format
//! 3. Implement `ConnectionStringFormatter` trait for output formats you need
//!
//! # Example
//!
//! ```rust
//! use crate::connector::connection_string::{PostgresConnectionString, ConnectionStringParser, ConnectionStringFormatter};
//!
//! let conn = PostgresConnectionString::parse("postgres://user:pass@localhost:5432/mydb").unwrap();
//! let duckdb_format = conn.to_duckdb_format();
//! ```

/// Error type for connection string parsing
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStringError {
    InvalidUrl(String),
    UnsupportedScheme(String),
    MissingHost,
    InvalidPort(String),
}

impl std::fmt::Display for ConnectionStringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStringError::InvalidUrl(msg) => write!(f, "Invalid URL: {}", msg),
            ConnectionStringError::UnsupportedScheme(scheme) => {
                write!(f, "Unsupported scheme: {}", scheme)
            }
            ConnectionStringError::MissingHost => write!(f, "Missing host in connection string"),
            ConnectionStringError::InvalidPort(msg) => write!(f, "Invalid port: {}", msg),
        }
    }
}

impl std::error::Error for ConnectionStringError {}

/// Trait for connection string formatting to different output formats.
/// Implement this trait to add support for new output dialects.
pub trait ConnectionStringFormatter {
    /// Format the connection string for DuckDB's postgres extension
    fn to_duckdb_format(&self) -> String;
}

/// Trait for parsing connection strings from URL format.
/// Implement this trait to add support for new database URL schemes.
pub trait ConnectionStringParser: Sized {
    /// The URL scheme this parser handles (e.g., "postgres", "mysql")
    fn scheme() -> &'static str;

    /// Parse a connection string URL into a structured format
    fn parse(connection_string: &str) -> Result<Self, ConnectionStringError>;
}

/// Represents a parsed PostgreSQL connection string
#[derive(Debug, Clone, PartialEq)]
pub struct PostgresConnectionString {
    pub user: Option<String>,
    pub password: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub dbname: Option<String>,
    pub options: Vec<(String, String)>,
}

impl ConnectionStringParser for PostgresConnectionString {
    fn scheme() -> &'static str {
        "postgres"
    }

    fn parse(connection_string: &str) -> Result<Self, ConnectionStringError> {
        let url = url::Url::parse(connection_string)
            .map_err(|e| ConnectionStringError::InvalidUrl(e.to_string()))?;

        // Validate scheme
        let scheme = url.scheme();
        if scheme != "postgres" && scheme != "postgresql" {
            return Err(ConnectionStringError::UnsupportedScheme(scheme.to_string()));
        }

        // Extract host
        let host = url
            .host_str()
            .ok_or(ConnectionStringError::MissingHost)?
            .to_string();

        // Extract user (empty string becomes None)
        let user = if url.username().is_empty() {
            None
        } else {
            Some(url.username().to_string())
        };

        // Extract password
        let password = url.password().map(|p| p.to_string());

        // Extract port
        let port = url.port();

        // Extract database name from path (remove leading slash)
        let dbname = {
            let path = url.path();
            if path.is_empty() || path == "/" {
                None
            } else {
                Some(path.trim_start_matches('/').to_string())
            }
        };

        // Extract query parameters as options
        let options: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Ok(PostgresConnectionString {
            user,
            password,
            host,
            port,
            dbname,
            options,
        })
    }
}

impl ConnectionStringFormatter for PostgresConnectionString {
    fn to_duckdb_format(&self) -> String {
        let mut parts = vec![format!("host={}", self.host)];

        if let Some(ref user) = self.user {
            parts.insert(0, format!("user={}", user));
        }
        if let Some(ref password) = self.password {
            parts.insert(
                if self.user.is_some() { 1 } else { 0 },
                format!("password={}", password),
            );
        }
        if let Some(port) = self.port {
            parts.push(format!("port={}", port));
        }
        if let Some(ref dbname) = self.dbname {
            parts.push(format!("dbname={}", dbname));
        }
        for (key, value) in &self.options {
            parts.push(format!("{}={}", key, value));
        }

        format!("postgres:{}", parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_connection_string() {
        let conn_str =
            "postgres://ducklake:ducklakepass@localhost:5432/ducklake_catalog?sslmode=disable";
        let parsed = PostgresConnectionString::parse(conn_str).unwrap();

        assert_eq!(parsed.user, Some("ducklake".to_string()));
        assert_eq!(parsed.password, Some("ducklakepass".to_string()));
        assert_eq!(parsed.host, "localhost".to_string());
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.dbname, Some("ducklake_catalog".to_string()));
        assert_eq!(
            parsed.options,
            vec![("sslmode".to_string(), "disable".to_string())]
        );
    }

    #[test]
    fn test_parse_connection_string_without_password() {
        let conn_str = "postgres://ducklake@localhost:5432/ducklake_catalog";
        let parsed = PostgresConnectionString::parse(conn_str).unwrap();

        assert_eq!(parsed.user, Some("ducklake".to_string()));
        assert_eq!(parsed.password, None);
        assert_eq!(parsed.host, "localhost".to_string());
        assert_eq!(parsed.port, Some(5432));
        assert_eq!(parsed.dbname, Some("ducklake_catalog".to_string()));
    }

    #[test]
    fn test_parse_connection_string_without_port() {
        let conn_str = "postgres://ducklake:ducklakepass@localhost/ducklake_catalog";
        let parsed = PostgresConnectionString::parse(conn_str).unwrap();

        assert_eq!(parsed.user, Some("ducklake".to_string()));
        assert_eq!(parsed.password, Some("ducklakepass".to_string()));
        assert_eq!(parsed.host, "localhost".to_string());
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.dbname, Some("ducklake_catalog".to_string()));
    }

    #[test]
    fn test_parse_connection_string_host_only() {
        let conn_str = "postgres://localhost";
        let parsed = PostgresConnectionString::parse(conn_str).unwrap();

        assert_eq!(parsed.user, None);
        assert_eq!(parsed.password, None);
        assert_eq!(parsed.host, "localhost".to_string());
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.dbname, None);
    }

    #[test]
    fn test_parse_connection_string_with_multiple_options() {
        let conn_str =
            "postgres://user:pass@localhost:5432/mydb?sslmode=require&connect_timeout=10";
        let parsed = PostgresConnectionString::parse(conn_str).unwrap();

        assert_eq!(parsed.options.len(), 2);
        assert!(
            parsed
                .options
                .contains(&("sslmode".to_string(), "require".to_string()))
        );
        assert!(
            parsed
                .options
                .contains(&("connect_timeout".to_string(), "10".to_string()))
        );
    }

    #[test]
    fn test_parse_invalid_url() {
        let conn_str = "not-a-valid-url";
        let result = PostgresConnectionString::parse(conn_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_wrong_scheme() {
        let conn_str = "mysql://user:pass@localhost:3306/mydb";
        let result = PostgresConnectionString::parse(conn_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_duckdb_format_full() {
        let conn = PostgresConnectionString {
            user: Some("ducklake".to_string()),
            password: Some("ducklakepass".to_string()),
            host: "localhost".to_string(),
            port: Some(5432),
            dbname: Some("ducklake_catalog".to_string()),
            options: vec![],
        };

        let result = conn.to_duckdb_format();
        assert_eq!(
            result,
            "postgres:user=ducklake password=ducklakepass host=localhost port=5432 dbname=ducklake_catalog"
        );
    }

    #[test]
    fn test_to_duckdb_format_minimal() {
        let conn = PostgresConnectionString {
            user: None,
            password: None,
            host: "localhost".to_string(),
            port: None,
            dbname: None,
            options: vec![],
        };

        let result = conn.to_duckdb_format();
        assert_eq!(result, "postgres:host=localhost");
    }

    #[test]
    fn test_to_duckdb_format_with_options() {
        let conn = PostgresConnectionString {
            user: Some("user".to_string()),
            password: Some("pass".to_string()),
            host: "localhost".to_string(),
            port: Some(5432),
            dbname: Some("mydb".to_string()),
            options: vec![("sslmode".to_string(), "disable".to_string())],
        };

        let result = conn.to_duckdb_format();
        assert_eq!(
            result,
            "postgres:user=user password=pass host=localhost port=5432 dbname=mydb sslmode=disable"
        );
    }

    #[test]
    fn test_roundtrip_parse_and_format() {
        let original =
            "postgres://ducklake:ducklakepass@localhost:5432/ducklake_catalog?sslmode=disable";
        let parsed = PostgresConnectionString::parse(original).unwrap();
        let formatted = parsed.to_duckdb_format();

        assert_eq!(
            formatted,
            "postgres:user=ducklake password=ducklakepass host=localhost port=5432 dbname=ducklake_catalog sslmode=disable"
        );
    }
}
