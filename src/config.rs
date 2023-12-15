use config::{Config as Cfg, ConfigError, Environment, File};
use serde::Deserialize;

/// Path to the file relative to the working directory of the TOML file containing the default
/// configuration for the application.
const DEFAULT_PATH: &str = "conf/default.toml";

/// Path to the file relative to the working directory of the TOML file containing the local
/// configuration for the application. This file is NOT committed to source control and will exist
/// only locally.
const LOCAL_PATH: &str = "conf/local.toml";

/// Enumerates the errors that can be generated from the `config` module.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Occurs when an error is encountered while initializing a [`Config`].
    #[error("error initializing the configuration")]
    InitializationError {
        #[from]
        source: ConfigError,
    },
}

/// The [`Http`] struct contains all of the configuration values related to the HTTP server.
#[derive(Debug, Deserialize)]
pub struct Http {
    /// Port that the HTTP server should listen on.
    pub port: u16,
}

/// The [`Database`] struct contains all of the configuration values related to the database that
/// the application connects to.
#[derive(Debug, Deserialize)]
pub struct Database {
    /// User used to connect to the database.
    pub user: String,
    /// Password used to connect to the database.
    pub password: String,
    /// URL used as the connection string to connect to the database.
    pub url: String,
    /// Name of the schema in the database for the application.
    pub name: String,
    /// Maximum number of connections allowed in the connection pool.
    pub max_connections: u32,
    /// Maximum number of seconds allowed to wait for a connection from the pool.
    pub connection_timeout: u64,
}

impl Database {
    /// Constructs the connection string used to connect to the database server based on the
    /// specified configuration.
    pub fn conn_str(&self) -> String {
        format!(
            "postgresql://{}:{}@{}/{}",
            self.user, self.password, self.url, self.name
        )
    }
}

/// The [`Kafka`] struct contains all of the configuration values related to producing and
/// consuming kafka events.
#[derive(Debug, Deserialize)]
pub struct Kafka {
    /// CSV of URLs used to connect to the cluster of Kafka brokers.
    pub servers: String,
}

/// The [`Outbox`] struct contains all of the configuration values related to publishing entries in
/// the `outbox` database table to Kafka.
#[derive(Debug, Deserialize)]
pub struct Outbox {
    /// Time in milliseconds between sweeps of the the outbox table.
    pub interval: u64,
    /// Maximum number of entries in the outbox table that should be processed in a single sweep.
    pub batch_size: u64,
}

/// The [`Config`] struct contains all of the available application configuration.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Key used for HMAC JWT signing when minting tokens and authenticating users. In a real
    /// application this would probably be a pointer to a key that is stored in a secure location
    /// like AWS Secrets Manager or similar rather than passing it in directly as an env variable.
    pub signing_key: String,
    /// HTTP configuration for the application.
    pub http: Http,
    /// Database configuration for the application.
    pub database: Database,
    /// Kafka configuration for the application.
    pub kafka: Kafka,
    /// Outbox configuration for the application.
    pub outbox: Outbox,
}

impl Config {
    /// Initializes a new [`Config`] by layering sources on top of each other. The following
    /// sources are layered and each subsequent one would override any values specified by the one
    /// before it.
    ///
    /// * `conf/default.toml` - Configuration file containing the default configuration values.
    /// * `conf/local.toml` - Optional configuration file that allows for env specific configuration.
    /// * Environment - Overlays any variables that begin with `RW_` from the runtime environment.
    pub fn init_from_env() -> Result<Self, Error> {
        let cfg = Cfg::builder()
            .add_source(File::with_name(DEFAULT_PATH))
            .add_source(File::with_name(LOCAL_PATH).required(false))
            .add_source(Environment::with_prefix("rw").separator("_"))
            .build()?;

        let config = cfg.try_deserialize()?;

        Ok(config)
    }
}

impl Default for Config {
    /// Creates a [`Config`] that is initialized with the default configuration.
    ///
    /// # Panics
    ///
    /// This function call will panic in the following situations.
    ///
    /// * Loading the default configuration file while building the [`Cfg`] fails
    /// * Deserialization of the builder into the [`Config`] struct fails
    fn default() -> Self {
        Cfg::builder()
            .add_source(File::with_name(DEFAULT_PATH))
            .build()
            .expect("failed to build default Config")
            .try_deserialize()
            .expect("failed to deserialize Config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the default [`Config`] is able to be successfully created and that the
    /// default values are correct.
    #[test]
    fn verify_default_configuration() {
        let config = Config::default();

        assert_eq!("default-signing-key", config.signing_key);

        assert_eq!(7100, config.http.port);

        assert_eq!("postgres", config.database.user);
        assert_eq!("", config.database.password);
        assert_eq!("localhost:5432", config.database.url);
        assert_eq!("postgres", config.database.name);
        assert_eq!(50, config.database.max_connections);
        assert_eq!(60, config.database.connection_timeout);

        assert_eq!("localhost:29092", config.kafka.servers);

        assert_eq!(1000, config.outbox.interval);
        assert_eq!(10, config.outbox.batch_size);
    }

    /// Verifies that a configured env variable correctly overrides the corresponding configuration
    /// value.
    #[test]
    fn verify_env_var_overlay_default() {
        std::env::set_var("RW_HTTP_PORT", "7300");

        let result = Config::init_from_env();
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(7300, config.http.port);

        std::env::remove_var("RW_HTTP_PORT")
    }
}
