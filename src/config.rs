use std::num::NonZeroUsize;
use std::path::PathBuf;

use ::config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct AppConfig {
    pub openai_api_key: String,
    pub chat_model: String,
    pub embedding_model: String,
    pub rag_db_path: PathBuf,
    pub rag_top_k: NonZeroUsize,
}

impl AppConfig {
    pub fn load() -> Result<Self, ::config::ConfigError> {
        Config::builder()
            .add_source(File::with_name("config"))
            .add_source(Environment::default().ignore_empty(true).try_parsing(true))
            .build()?
            .try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ::config::FileFormat;

    use super::*;

    #[test]
    fn environment_overrides_yaml() {
        let env = HashMap::from([
            ("OPENAI_API_KEY".to_string(), "sk-demo".to_string()),
            ("CHAT_MODEL".to_string(), "gpt-test".to_string()),
            ("RAG_TOP_K".to_string(), "5".to_string()),
        ]);

        let config: AppConfig = Config::builder()
            .add_source(File::from_str(
                r#"
                chat_model: gpt-yaml
                embedding_model: text-embedding-3-small
                rag_db_path: data/test.sqlite
                rag_top_k: 3
                "#,
                FileFormat::Yaml,
            ))
            .add_source(Environment::default().try_parsing(true).source(Some(env)))
            .build()
            .expect("test config sources should build")
            .try_deserialize()
            .expect("valid config should deserialize");

        assert_eq!(config.openai_api_key, "sk-demo");
        assert_eq!(config.chat_model, "gpt-test");
        assert_eq!(config.rag_top_k.get(), 5);
    }

    #[test]
    fn missing_openai_key_fails() {
        let result = Config::builder()
            .add_source(File::from_str(
                r#"
                chat_model: gpt-test
                embedding_model: text-embedding-3-small
                rag_db_path: data/test.sqlite
                rag_top_k: 3
                "#,
                FileFormat::Yaml,
            ))
            .build()
            .expect("test config source should build")
            .try_deserialize::<AppConfig>();

        assert!(result.is_err());
    }
}
