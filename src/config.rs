//! Runtime configuration for the retail workflow.

use std::path::PathBuf;

use ::config::{Config, Environment, File};
use serde::Deserialize;

/// Retail workflow runtime configuration.
#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    /// Optional provider key used only by decision commands in later branches.
    #[serde(default)]
    pub openai_api_key: Option<String>,
    /// Chat model used by the Rig-backed decision agent in later branches.
    pub chat_model: String,
    /// `SQLite` database path for durable retail state in later branches.
    pub retail_db_path: PathBuf,
    /// Scenario YAML path used by the seed command in later branches.
    pub retail_scenario_path: PathBuf,
    /// Default demand horizon for restock decisions.
    pub decision_horizon_days: u64,
    /// Maximum accepted restock orders per decision.
    pub max_restock_orders_per_decision: u64,
}

impl AppConfig {
    /// Load runtime configuration from `config.yaml` and environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error when required common configuration is missing or invalid.
    pub fn load() -> Result<Self, ::config::ConfigError> {
        Self::from_builder(
            Config::builder()
                .add_source(File::with_name("config"))
                .add_source(Environment::default().ignore_empty(true).try_parsing(true)),
        )
    }

    fn from_builder(
        builder: ::config::ConfigBuilder<::config::builder::DefaultState>,
    ) -> Result<Self, ::config::ConfigError> {
        builder.build()?.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ::config::FileFormat;

    use super::AppConfig;

    #[test]
    fn environment_overrides_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let env = HashMap::from([
            ("OPENAI_API_KEY".to_owned(), "sk-demo".to_owned()),
            ("CHAT_MODEL".to_owned(), "gpt-test".to_owned()),
            ("DECISION_HORIZON_DAYS".to_owned(), "21".to_owned()),
        ]);

        let config = AppConfig::from_builder(
            ::config::Config::builder()
                .add_source(::config::File::from_str(
                    r"
                    chat_model: gpt-yaml
                    retail_db_path: data/test.sqlite
                    retail_scenario_path: data/retail_scenario.yaml
                    decision_horizon_days: 14
                    max_restock_orders_per_decision: 2
                    ",
                    FileFormat::Yaml,
                ))
                .add_source(
                    ::config::Environment::default()
                        .try_parsing(true)
                        .source(Some(env)),
                ),
        )?;

        assert_eq!(config.openai_api_key, Some("sk-demo".to_owned()));
        assert_eq!(config.chat_model, "gpt-test");
        assert_eq!(config.decision_horizon_days, 21);
        Ok(())
    }

    #[test]
    fn missing_openai_key_still_loads_common_config() -> Result<(), Box<dyn std::error::Error>> {
        let config = AppConfig::from_builder(::config::Config::builder().add_source(
            ::config::File::from_str(
                r"
                chat_model: gpt-test
                retail_db_path: data/test.sqlite
                retail_scenario_path: data/retail_scenario.yaml
                decision_horizon_days: 14
                max_restock_orders_per_decision: 2
                ",
                FileFormat::Yaml,
            ),
        ))?;

        assert_eq!(config.openai_api_key, None);
        Ok(())
    }
}
