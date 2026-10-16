use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use trellis_land::{Engine, Env, Policy, SystemEnv};
use trellis_store::{BlobStore, FsBlobStore, MetaStore, SqliteMetaStore};
use trellis_validate::{Runner, ValidatorSpec};

use crate::BoxError;

pub struct Config {
    pub addr: SocketAddr,
    pub data_dir: PathBuf,
    pub policy: Policy,
    /// Checks run after landings (spec §4.7).
    pub validators: Vec<ValidatorSpec>,
    /// How many unfinished tasks one principal may hold at once.
    pub max_claims: usize,
}

impl Config {
    pub fn new(addr: SocketAddr, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            addr,
            data_dir: data_dir.into(),
            policy: Policy::default(),
            validators: Vec::new(),
            max_claims: 1,
        }
    }
}

/// The shared stores and one engine per repository, created on first use.
pub struct AppState {
    meta: Arc<dyn MetaStore>,
    blobs: Arc<dyn BlobStore>,
    env: Arc<dyn Env>,
    policy: Policy,
    engines: Mutex<HashMap<String, Arc<Engine>>>,
    validators: Vec<ValidatorSpec>,
    data_dir: PathBuf,
    max_claims: usize,
}

impl AppState {
    pub fn open(config: &Config) -> Result<Self, BoxError> {
        std::fs::create_dir_all(&config.data_dir)?;
        Ok(Self {
            meta: Arc::new(SqliteMetaStore::open(config.data_dir.join("meta.db"))?),
            blobs: Arc::new(FsBlobStore::open(config.data_dir.join("objects"))?),
            env: Arc::new(SystemEnv::new()),
            policy: config.policy.clone(),
            engines: Mutex::new(HashMap::new()),
            validators: config.validators.clone(),
            data_dir: config.data_dir.clone(),
            max_claims: config.max_claims.max(1),
        })
    }

    /// Runs the configured validators for every active repository, about twice a second,
    /// until `stop` is set. Does nothing without validators.
    pub fn spawn_validators(self: &Arc<Self>, stop: Arc<AtomicBool>) {
        if self.validators.is_empty() {
            return;
        }
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let mut runners: HashMap<String, Runner> = HashMap::new();
            while !stop.load(Ordering::Relaxed) {
                let engines: Vec<(String, Arc<Engine>)> = state
                    .engines
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .iter()
                    .map(|(repo, engine)| (repo.clone(), Arc::clone(engine)))
                    .collect();
                for (repo, engine) in engines {
                    if engine.main().is_err() {
                        continue;
                    }
                    let runner = runners.entry(repo.clone()).or_insert_with(|| {
                        Runner::new(
                            engine,
                            state.validators.clone(),
                            state.data_dir.join("worktrees").join(&repo),
                        )
                    });
                    if let Err(e) = runner.run_pending() {
                        eprintln!("trellis: validators for {repo}: {e}");
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    pub fn engine(&self, repo: &str) -> Arc<Engine> {
        let mut engines = self
            .engines
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(engines.entry(repo.to_owned()).or_insert_with(|| {
            Arc::new(
                Engine::new(
                    repo,
                    Arc::clone(&self.meta),
                    Arc::clone(&self.blobs),
                    Arc::clone(&self.env),
                    self.policy.clone(),
                )
                .with_max_claims(self.max_claims),
            )
        }))
    }
}
