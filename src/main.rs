//! The incisive server and admin commands.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use incisive::api::{self, AppState};
use incisive::config::Config;
use incisive::meta::{self, Meta};
use incisive::store::{self, objstore, Engine};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "incisive", version, about = "Event analytics server on object storage")]
struct Cli {
    /// Config file (TOML). Environment variables INCISIVE_* (with __ for nesting) override it.
    #[arg(short, long, env = "INCISIVE_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the HTTP server.
    Serve,
    /// Create a project and print its keys.
    CreateProject {
        name: String,
        /// Add the project to this user's account (created if needed).
        #[arg(long)]
        owner: Option<String>,
    },
    /// Create an account master key for a user and print it.
    CreateAccountKey {
        name: String,
        /// The user's email (created if needed).
        #[arg(long)]
        owner: String,
    },
    /// Compact all pending WAL objects into segments now.
    Compact,
    /// Back up the metadata database to object storage now.
    Backup,
    /// Print the effective configuration.
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref())?;
    match cli.cmd {
        Cmd::Config => {
            println!("{}", toml_like(&cfg));
            Ok(())
        }
        Cmd::Serve => serve(cfg).await,
        Cmd::CreateProject { name, owner } => {
            let (engine, meta) = open(&cfg).await?;
            let uid = match owner {
                Some(email) => Some(meta.ensure_user(&email.to_lowercase())?.0.id),
                None => None,
            };
            let (p, keys) = meta.create_project(uid.as_deref(), &name)?;
            println!("project_id: {}", p.id);
            for (k, secret) in keys {
                println!("{}: {secret}", k.name);
            }
            meta::backup_once(&meta, &engine, &mut None).await?;
            Ok(())
        }
        Cmd::CreateAccountKey { name, owner } => {
            let (engine, meta) = open(&cfg).await?;
            let (user, _) = meta.ensure_user(&owner.to_lowercase())?;
            let (_, secret) = meta.create_account_key(&user.id, &name)?;
            println!("{secret}");
            meta::backup_once(&meta, &engine, &mut None).await?;
            Ok(())
        }
        Cmd::Compact => {
            let (engine, _) = open(&cfg).await?;
            let n = engine.compact_all(true).await?;
            println!("compacted {n} collections");
            Ok(())
        }
        Cmd::Backup => {
            let (engine, meta) = open(&cfg).await?;
            meta::backup_once(&meta, &engine, &mut None).await?;
            println!("backed up to {}", meta::BACKUP_LATEST);
            Ok(())
        }
    }
}

fn toml_like(cfg: &Config) -> String {
    serde_json::to_string_pretty(cfg).unwrap_or_default()
}

/// Opens storage, the engine and metadata, restoring metadata from the latest backup when the
/// local database is missing.
async fn open(cfg: &Config) -> Result<(Arc<Engine>, Arc<Meta>)> {
    let store = objstore::build(&cfg.storage)?;
    let engine = Engine::open(cfg, store).await.context("opening storage engine")?;
    if !cfg.meta.path.exists()
        && let Some(snap) = engine.get_object(meta::BACKUP_LATEST).await? {
            Meta::restore_if_missing(&cfg.meta.path, &snap)?;
            tracing::info!(path = %cfg.meta.path.display(), "restored metadata from backup");
        }
    let meta = Meta::open(&cfg.meta.path)?;
    Ok((engine, meta))
}

async fn serve(cfg: Config) -> Result<()> {
    let (engine, meta) = open(&cfg).await?;
    let st = Arc::new(AppState::new(cfg.clone(), engine.clone(), meta.clone())?);
    if cfg.compaction.enabled {
        tokio::spawn(store::compaction_loop(engine.clone()));
    }
    tokio::spawn(meta::backup_loop(meta.clone(), engine.clone(), cfg.meta.backup_interval_secs));
    let app = api::router(st.clone()).merge(incisive::web::router(st.clone())).layer(
        tower_http::trace::TraceLayer::new_for_http(),
    );
    let listener = tokio::net::TcpListener::bind(&cfg.server.addr).await.with_context(|| format!("binding {}", cfg.server.addr))?;
    tracing::info!(addr = %cfg.server.addr, backend = %cfg.storage.backend, prefix = %cfg.storage.prefix, "listening");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown())
        .await?;
    // Requests are acknowledged only after their events are durable, so there is nothing to
    // flush; take a last metadata backup.
    if let Err(e) = meta::backup_once(&meta, &engine, &mut None).await {
        tracing::warn!(error = %e, "final metadata backup failed");
    }
    tracing::info!("stopped");
    Ok(())
}

async fn shutdown() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signal handler");
    tokio::select! {
        _ = ctrl_c => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutting down");
}
