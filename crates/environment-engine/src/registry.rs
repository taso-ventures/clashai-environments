//! Environment registry for looking up factory functions by environment type.
//!
//! The [`EnvironmentRegistry`] stores factory closures keyed by environment-type string
//! (e.g. `"coup"`). Callers provide an [`EnvironmentConfig`] and receive a boxed
//! [`Environment`] instance.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{Environment, EnvironmentConfig, EnvironmentError, PostMatchVisibility, Result};

/// Factory function that creates a new environment instance.
pub type EnvironmentFactory =
    Arc<dyn Fn(&EnvironmentConfig) -> Result<Box<dyn Environment>> + Send + Sync>;

/// Static metadata for an environment type, registered alongside the factory.
///
/// This allows querying environment properties (player bounds, display name,
/// visibility policy) without constructing a probe instance.
#[derive(Debug, Clone)]
pub struct EnvironmentMeta {
    pub display_name: &'static str,
    pub min_players: usize,
    pub max_players: usize,
    pub post_match_visibility: PostMatchVisibility,
}

/// Registry of environment factories keyed by environment type identifier.
///
/// `EnvironmentRegistry` is **not** internally synchronized. It is designed to be
/// initialized once at startup (via [`EnvironmentRegistry::with_defaults`]) and then
/// shared immutably across threads wrapped in an [`Arc`]. Since [`create`](EnvironmentRegistry::create)
/// takes `&self`, no additional locking is required for concurrent reads.
///
/// # Example
///
/// ```rust,ignore
/// let registry = Arc::new(EnvironmentRegistry::with_defaults());
///
/// // Pass `registry` to multiple tasks / threads.
/// let engine = registry.create("coup", &config)?;
/// ```
#[derive(Clone)]
pub struct EnvironmentRegistry {
    factories: HashMap<String, EnvironmentFactory>,
    metadata: HashMap<String, EnvironmentMeta>,
}

impl EnvironmentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Register a factory and static metadata for the given environment type.
    ///
    /// Overwrites any previously registered factory for that type.
    pub fn register(
        &mut self,
        environment_type: &str,
        meta: EnvironmentMeta,
        factory: EnvironmentFactory,
    ) {
        self.factories.insert(environment_type.to_string(), factory);
        self.metadata.insert(environment_type.to_string(), meta);
    }

    /// Create an environment instance by looking up the factory for `environment_type`.
    pub fn create(
        &self,
        environment_type: &str,
        config: &EnvironmentConfig,
    ) -> Result<Box<dyn Environment>> {
        let factory = self.factories.get(environment_type).ok_or_else(|| {
            EnvironmentError::InvalidSetup(format!("unknown environment type: {environment_type}"))
        })?;
        factory(config)
    }

    /// Get static metadata for an environment type without constructing an instance.
    pub fn get_meta(&self, environment_type: &str) -> Option<&EnvironmentMeta> {
        self.metadata.get(environment_type)
    }

    /// Return a list of registered environment type identifiers.
    pub fn available_environments(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }

    /// Create a registry pre-populated with all compiled-in environments.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        #[cfg(feature = "coup")]
        {
            use crate::coup::CoupEnvironment;
            registry.register(
                "coup",
                EnvironmentMeta {
                    display_name: "Coup",
                    min_players: 2,
                    max_players: 6,
                    post_match_visibility: PostMatchVisibility::OwnOnly,
                },
                Arc::new(|config| {
                    Ok(Box::new(CoupEnvironment::new(
                        config.player_count,
                        config.seed,
                    )?))
                }),
            );
        }

        #[cfg(feature = "vibe_check")]
        {
            use crate::vibe_check::VibeCheckEnvironment;
            registry.register(
                "vibe_check",
                EnvironmentMeta {
                    display_name: "Wavelength",
                    min_players: 4,
                    max_players: 6,
                    post_match_visibility: PostMatchVisibility::OwnOnly,
                },
                Arc::new(|config| {
                    Ok(Box::new(VibeCheckEnvironment::new(
                        config.player_count,
                        config.seed,
                    )?))
                }),
            );
        }

        #[cfg(feature = "red_button")]
        {
            use crate::red_button::RedButtonEnvironment;
            use red_button_protocol::RedButtonConfig;
            registry.register(
                "red_button",
                EnvironmentMeta {
                    display_name: "Red Button",
                    min_players: 2,
                    max_players: 2,
                    post_match_visibility: PostMatchVisibility::Full,
                },
                Arc::new(|config| {
                    let rb_config: RedButtonConfig = serde_json::to_value(&config.extra)
                        .ok()
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default();

                    let player_ids: Vec<i32> = config
                        .player_ids
                        .clone()
                        .unwrap_or_else(|| (0..config.player_count as i32).collect());

                    let player_names = config.player_names.clone().unwrap_or_default();

                    let match_id = config
                        .match_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());

                    Ok(Box::new(RedButtonEnvironment::new(
                        match_id,
                        player_ids,
                        player_names,
                        rb_config,
                    )?))
                }),
            );
        }

        #[cfg(feature = "tic_tac_toe")]
        {
            use crate::tic_tac_toe::TicTacToeEnvironment;
            registry.register(
                "tic_tac_toe",
                EnvironmentMeta {
                    display_name: "Tic-Tac-Toe",
                    min_players: 2,
                    max_players: 2,
                    post_match_visibility: PostMatchVisibility::Full,
                },
                Arc::new(|config| {
                    let player_ids: Vec<i32> = config
                        .player_ids
                        .clone()
                        .unwrap_or_else(|| (0..config.player_count as i32).collect());

                    let player_names = config.player_names.clone().unwrap_or_default();

                    let match_id = config
                        .match_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());

                    Ok(Box::new(TicTacToeEnvironment::new(
                        match_id,
                        player_ids,
                        player_names,
                    )?))
                }),
            );
        }

        #[cfg(feature = "connect_four")]
        {
            use crate::connect_four::ConnectFourEnvironment;
            registry.register(
                "connect_four",
                EnvironmentMeta {
                    display_name: "Connect Four",
                    min_players: 2,
                    max_players: 2,
                    post_match_visibility: PostMatchVisibility::Full,
                },
                Arc::new(|config| {
                    let player_ids: Vec<i32> = config
                        .player_ids
                        .clone()
                        .unwrap_or_else(|| (0..config.player_count as i32).collect());

                    let player_names = config.player_names.clone().unwrap_or_default();

                    let match_id = config
                        .match_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());

                    Ok(Box::new(ConnectFourEnvironment::new(
                        match_id,
                        player_ids,
                        player_names,
                    )?))
                }),
            );
        }

        #[cfg(feature = "wordle")]
        {
            use crate::wordle::WordleEnvironment;
            use wordle_protocol::WordleConfig;
            registry.register(
                "wordle",
                EnvironmentMeta {
                    display_name: "Wordle",
                    min_players: 3,
                    max_players: 6,
                    post_match_visibility: PostMatchVisibility::Full,
                },
                Arc::new(|config| {
                    let w_config: WordleConfig = match serde_json::to_value(&config.extra)
                        .ok()
                        .and_then(|v| serde_json::from_value(v).ok())
                    {
                        Some(cfg) => cfg,
                        None => {
                            tracing::warn!("invalid wordle config.extra, using defaults");
                            WordleConfig::default()
                        }
                    };

                    let player_ids: Vec<i32> = config
                        .player_ids
                        .clone()
                        .unwrap_or_else(|| (0..config.player_count as i32).collect());

                    let player_names = config.player_names.clone().unwrap_or_default();

                    let match_id = config
                        .match_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());

                    Ok(Box::new(WordleEnvironment::new(
                        match_id,
                        player_ids,
                        player_names,
                        w_config,
                        config.seed,
                    )?))
                }),
            );
        }

        #[cfg(feature = "poker")]
        {
            use crate::poker::PokerEnvironment;
            registry.register(
                "poker",
                EnvironmentMeta {
                    display_name: "Poker",
                    min_players: 2,
                    max_players: 2,
                    post_match_visibility: PostMatchVisibility::OwnOnly,
                },
                Arc::new(|config| Ok(Box::new(PokerEnvironment::new(config.seed)?))),
            );
        }

        registry
    }
}

impl Default for EnvironmentRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
