use std::io;

#[derive(Debug, thiserror::Error)]
pub enum CcrlError {
    #[error("Config file not found: {0}")]
    ConfigNotFound(String),

    #[error("Profile '{0}' not found in config")]
    ProfileNotFound(String),

    #[error("Environment variable '{0}' not set")]
    EnvVarNotSet(String),

    #[error("Invalid color '{0}'. Supported: red, green, yellow, blue, magenta, cyan, white, black, or #RRGGBB hex")]
    InvalidColor(String),

    #[error("Invalid config: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Invalid settings.json: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Invalid auth.json: {0}")]
    AuthJsonParse(String),

    #[error("Unsupported target '{0}'")]
    UnsupportedTarget(String),

    #[error("No OAuth auth snapshot is available")]
    OAuthSnapshotMissing,

    #[error("{0}")]
    Io(#[from] io::Error),
}
