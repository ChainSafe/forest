// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::{ADMIN, create_token, generate_priv_key};
use crate::chain::ChainStore;
use crate::cli_shared::chain_path;
use crate::cli_shared::cli::CliOpts;
use crate::daemon::asyncify;
use crate::daemon::bundle::load_actor_bundles;
use crate::daemon::db_util::load_all_forest_cars_with_cleanup;
use crate::db::car::ManyCar;
use crate::db::db_engine::{db_root, open_db};
use crate::db::parity_db::ParityDb;
use crate::db::{CAR_DB_DIR_NAME, DummyStore, EthMappingsStore};
use crate::genesis::read_genesis_header;
use crate::libp2p::{Keypair, PeerId};
use crate::networks::ChainConfig;
use crate::rpc::sync::SnapshotProgressTracker;
use crate::shim::address::CurrentNetwork;
use crate::state_manager::StateManager;
use crate::{
    Config, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV, JWT_IDENTIFIER, KeyStore,
    KeyStoreConfig,
};
use anyhow::Context;
use dialoguer::console::Term;
use fvm_shared4::address::Network;
use parking_lot::RwLock;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

pub struct AppContext {
    pub net_keypair: Keypair,
    pub p2p_peer_id: PeerId,
    pub db: Arc<DbType>,
    pub db_meta_data: DbMetadata,
    pub state_manager: Arc<StateManager<DbType>>,
    pub keystore: Arc<RwLock<KeyStore>>,
    pub admin_jwt: String,
    pub snapshot_progress_tracker: SnapshotProgressTracker,
}

impl AppContext {
    pub async fn init(opts: &CliOpts, cfg: &Config) -> anyhow::Result<AppContext> {
        let chain_cfg = get_chain_config_and_set_network(cfg);
        let (net_keypair, p2p_peer_id) = get_or_create_p2p_keypair_and_peer_id(cfg)?;
        let (db, db_meta_data) = setup_db(opts, cfg).await?;
        let state_manager = create_state_manager(cfg, &db, &chain_cfg).await?;
        let (keystore, admin_jwt) = load_or_create_keystore_and_configure_jwt(opts, cfg).await?;
        let snapshot_progress_tracker = SnapshotProgressTracker::default();
        Ok(Self {
            net_keypair,
            p2p_peer_id,
            db,
            db_meta_data,
            state_manager,
            keystore,
            admin_jwt,
            snapshot_progress_tracker,
        })
    }

    pub fn chain_config(&self) -> &Arc<ChainConfig> {
        self.state_manager.chain_config()
    }

    pub fn chain_store(&self) -> &Arc<ChainStore<DbType>> {
        self.state_manager.chain_store()
    }
}

fn get_chain_config_and_set_network(config: &Config) -> Arc<ChainConfig> {
    let chain_config = ChainConfig::from_chain(config.chain());
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    Arc::new(ChainConfig {
        enable_indexer: config.chain_indexer.enable_indexer,
        enable_receipt_event_caching: config.client.enable_rpc,
        default_max_fee: config.fee.max_fee.clone(),
        ..chain_config
    })
}

fn get_or_create_p2p_keypair_and_peer_id(config: &Config) -> anyhow::Result<(Keypair, PeerId)> {
    let path = config.client.data_dir.join("libp2p");
    let keypair = crate::libp2p::keypair::get_or_create_keypair(&path)?;
    let peer_id = keypair.public().to_peer_id();
    Ok((keypair, peer_id))
}

/// This may:
/// - create a [`KeyStore`]
/// - load a [`KeyStore`]
/// - ask a user for password input
async fn load_or_create_keystore(config: &Config) -> anyhow::Result<KeyStore> {
    use std::env::VarError;

    let passphrase_from_env = std::env::var(FOREST_KEYSTORE_PHRASE_ENV);
    let require_encryption = config.client.encrypt_keystore;
    let keystore_already_exists = config
        .client
        .data_dir
        .join(ENCRYPTED_KEYSTORE_NAME)
        .is_dir();

    match (require_encryption, passphrase_from_env) {
        // don't need encryption, we can implicitly create a keystore
        (false, maybe_passphrase) => {
            warn!("Forest has encryption disabled");
            if let Ok(_) | Err(VarError::NotUnicode(_)) = maybe_passphrase {
                warn!(
                    "Ignoring passphrase provided in {} - encryption is disabled",
                    FOREST_KEYSTORE_PHRASE_ENV
                )
            }
            KeyStore::new(KeyStoreConfig::Persistent(config.client.data_dir.clone()))
                .map_err(anyhow::Error::new)
        }

        // need encryption, the user has provided the password through env
        (true, Ok(passphrase)) => KeyStore::new(KeyStoreConfig::Encrypted(
            config.client.data_dir.clone(),
            passphrase,
        ))
        .map_err(anyhow::Error::new),

        // need encryption, we've not been given a password
        (true, Err(error)) => {
            // prompt for passphrase and try and load the keystore

            if let VarError::NotUnicode(_) = error {
                // If we're ignoring the user's password, tell them why
                warn!(
                    "Ignoring passphrase provided in {} - it's not utf-8",
                    FOREST_KEYSTORE_PHRASE_ENV
                )
            }

            let data_dir = config.client.data_dir.clone();

            match keystore_already_exists {
                true => asyncify(move || input_password_to_load_encrypted_keystore(data_dir))
                    .await
                    .context("Couldn't load keystore"),
                false => {
                    let password =
                        asyncify(|| create_password("Create a password for Forest's keystore"))
                            .await?;
                    KeyStore::new(KeyStoreConfig::Encrypted(data_dir, password))
                        .context("Couldn't create keystore")
                }
            }
        }
    }
}

async fn load_or_create_keystore_and_configure_jwt(
    opts: &CliOpts,
    config: &Config,
) -> anyhow::Result<(Arc<RwLock<KeyStore>>, String)> {
    let mut keystore = load_or_create_keystore(config).await?;
    if keystore.get(JWT_IDENTIFIER).is_err() {
        keystore.put(JWT_IDENTIFIER, generate_priv_key())?;
    }
    let admin_jwt = handle_admin_token(opts, config, &keystore)?;
    let keystore = Arc::new(RwLock::new(keystore));
    Ok((keystore, admin_jwt))
}

fn maybe_migrate_db(config: &Config) {
    // Try to migrate the database if needed. In case the migration fails, we fallback to creating a new database
    // to avoid breaking the node.
    let db_migration = crate::db::migration::DbMigration::new(config);
    if let Err(e) = db_migration.migrate() {
        warn!("Failed to migrate database: {e}");
    }
}

pub type DbType = ManyCar<Arc<ParityDb>>;

pub(crate) struct DbMetadata {
    db_root_dir: PathBuf,
    forest_car_db_dir: PathBuf,
}

impl DbMetadata {
    pub(crate) fn get_root_dir(&self) -> PathBuf {
        self.db_root_dir.clone()
    }

    pub(crate) fn get_forest_car_db_dir(&self) -> PathBuf {
        self.forest_car_db_dir.clone()
    }
}

/// This function configures database with below steps
/// - migrate database auto-magically on Forest version bump
/// - load parity-db
/// - load CAR database
/// - load actor bundles
async fn setup_db(opts: &CliOpts, config: &Config) -> anyhow::Result<(Arc<DbType>, DbMetadata)> {
    maybe_migrate_db(config);
    let chain_data_path = chain_path(config);
    let db_root_dir = db_root(&chain_data_path)?;
    let db_writer = Arc::new(open_db(db_root_dir.clone(), config.db_config())?);
    let db = Arc::new(ManyCar::new(db_writer.clone()));
    let forest_car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
    load_all_forest_cars_with_cleanup(&db, &forest_car_db_dir)?;
    if config.client.load_actors && !opts.stateless {
        load_actor_bundles(&db, config.chain()).await?;
    }
    Ok((
        db,
        DbMetadata {
            db_root_dir,
            forest_car_db_dir,
        },
    ))
}

async fn create_state_manager(
    config: &Config,
    db: &Arc<DbType>,
    chain_config: &Arc<ChainConfig>,
) -> anyhow::Result<Arc<StateManager<DbType>>> {
    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be
    //   initialized
    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_deref(),
        chain_config.genesis_bytes(db).await?.as_deref(),
        db,
    )
    .await?;

    let eth_mappings: Arc<dyn EthMappingsStore + Sync + Send> =
        if config.chain_indexer.enable_indexer {
            db.writer().clone()
        } else {
            Arc::new(DummyStore {})
        };
    let chain_store = Arc::new(ChainStore::new(
        Arc::clone(db),
        Arc::new(db.clone()),
        eth_mappings,
        chain_config.clone(),
        genesis_header.clone(),
    )?);

    // Initialize StateManager
    let state_manager = Arc::new(StateManager::new(Arc::clone(&chain_store))?);

    Ok(state_manager)
}

/// Prompts for password, looping until the [`KeyStore`] is successfully loaded.
///
/// This code makes blocking syscalls.
fn input_password_to_load_encrypted_keystore(data_dir: PathBuf) -> dialoguer::Result<KeyStore> {
    let keystore = RefCell::new(None);
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        )
        .into());
    }

    dialoguer::Password::new()
        .with_prompt("Enter the password for Forest's keystore")
        .allow_empty_password(true) // let validator do validation
        .validate_with(|input: &String| {
            KeyStore::new(KeyStoreConfig::Encrypted(data_dir.clone(), input.clone()))
                .map(|created| *keystore.borrow_mut() = Some(created))
                .context(
                    "Error: couldn't load keystore with this password. Try again or press Ctrl+C to abort.",
                )
        })
        .interact_on(&term)?;

    Ok(keystore
        .into_inner()
        .expect("validation succeeded, so keystore must be emplaced"))
}

/// Loops until the user provides two matching passwords.
///
/// This code makes blocking syscalls
fn create_password(prompt: &str) -> dialoguer::Result<String> {
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        )
        .into());
    }
    dialoguer::Password::new()
        .with_prompt(prompt)
        .allow_empty_password(false)
        .with_confirmation(
            "Confirm password",
            "Error: the passwords do not match. Try again or press Ctrl+C to abort.",
        )
        .interact_on(&term)
}

/// Generates, prints and optionally writes to a file the administrator JWT
/// token.
fn handle_admin_token(
    opts: &CliOpts,
    config: &Config,
    keystore: &KeyStore,
) -> anyhow::Result<String> {
    let ki = keystore.get(JWT_IDENTIFIER)?;
    // Lotus admin tokens do not expire but Forest requires all JWT tokens to
    // have an expiration date. So we set the expiration date to 100 years in
    // the future to match user-visible behavior of Lotus.
    let token_exp = chrono::Duration::days(365 * 100);
    let token = create_token(
        ADMIN.iter().map(ToString::to_string).collect(),
        ki.private_key(),
        token_exp,
    )?;
    let default_token_path = config.client.default_rpc_token_path();
    if let Err(e) =
        crate::utils::io::write_new_sensitive_file(token.as_bytes(), &default_token_path)
    {
        tracing::warn!("Failed to save the default admin token file: {e}");
    } else {
        info!("Admin token is saved to {}", default_token_path.display());
    }
    if let Some(path) = opts.save_token.as_ref() {
        if let Some(dir) = path.parent()
            && !dir.is_dir()
        {
            std::fs::create_dir_all(dir).with_context(|| {
                format!(
                    "Failed to create `--save-token` directory {}",
                    dir.display()
                )
            })?;
        }
        std::fs::write(path, &token)
            .with_context(|| format!("Failed to save admin token to {}", path.display()))?;
        info!("Admin token is saved to {}", path.display());
    }

    Ok(token)
}
