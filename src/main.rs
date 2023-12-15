use realworld::config::Config;
use realworld::event;
use realworld::http;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;

/// The main entry point into the application.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A real production application would want to prefer structured logging, e.g. json formatted,
    // but the pretty configuration allows for readability when developing locally and will be fine
    // for this project. We default to INFO logs but allow the RUST_LOG env variable to override.
    tracing_subscriber::fmt()
        .pretty()
        .with_level(true)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Convenient way to allow developers to easily override configuration during local
    // development by simply putting env variables in a .env file that is excluded from git.
    // We also allow for a conf/local.toml file to specify local configuration as well. Both are
    // not really necessary but show available options.
    match dotenvy::dotenv_override() {
        Ok(path) => tracing::debug!("loaded .env file from {}", path.to_string_lossy()),
        Err(e) => tracing::debug!("unable to load .env file: {}", e),
    };

    // Initialize the configuration from the layered sources. Custom configuration can be added by
    // adding configuration to the conf/local.toml file, the .env file at the root dir or by
    // setting corresponding environment variables at runtime with the RW_ prefix.
    let config = Arc::new(Config::init_from_env()?);

    // Create the connection pool that will be used to interact with the backend database. In a
    // real application the user would want to tweak the available parameters based on the expected
    // load and expose other relevant parameters through the configuration so that they may be
    // tweaked without a code change.
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .acquire_timeout(Duration::from_secs(config.database.connection_timeout))
        .connect(&config.database.conn_str())
        .await?;

    // Run any required SQL migrations contained in the migrations folder that have not yet run
    // against the database before we start listening for HTTP connections.
    sqlx::migrate!().run(&pool).await?;

    // Start the outbox processing task
    let outbox_fut = event::start_outbox_processor(pool.clone(), Arc::clone(&config));

    // Configure the routes for the application and start the HTTP server on the configured port.
    let tcp_listener =
        tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.http.port)).await?;

    let http_fut =
        async { axum::serve(tcp_listener, http::router(pool, Arc::clone(&config))).await };

    // If running on a unix system, install a handler for the terminate signal so we can cleanly
    // shutdown. If not running on a unix system then instead use a future that will never return.
    #[cfg(unix)]
    let terminate_signal = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to setup hook for terminate signal")
            .recv()
            .await
    };

    #[cfg(not(unix))]
    let shutdown_signal = futures::future::pending::<()>();

    // Install a handler for the ctrl + c key combination so we can cleanly shutdown if a user
    // manually closes the application through the terminal.
    let ctrl_c_signal = tokio::signal::ctrl_c();

    // Execute all of the futures and return when one of them completes. Ideally only the signal
    // handlers would be the ones that complete as any other case would generally indicate an
    // error that would cause the application to exit.
    tokio::select! {
        http_res = http_fut => {
            if let Err(e) = http_res {
                tracing::error!("error while running HTTP server: {}", e);
            }
        }
        outbox_res = outbox_fut => {
            if let Err(e) = outbox_res {
                tracing::error!("error while processing outbox: {}", e);
            }
        }
        _ = terminate_signal => {
            tracing::info!("received shutdown signal");
        }
        _ = ctrl_c_signal => {
            tracing::info!("received ctrl+c signal");
        }
    }

    tracing::info!("application has shutdown");

    Ok(())
}
