use std::env;
use config::{Config, ConfigError, Environment, File, FileFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct Database {
    user: String,
    password: String,
    host: String,
    port: String,
    database: String,
}

impl Database {
    
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub(crate) struct Settings {
    database: Database,
}

impl Settings {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .set_default("user", "typednotes")?
            .set_default("password", "password")?
            .set_default("host", "localhost")?
            .set_default("port", "5432")?
            .set_default("database", "typednotes")?
            .add_source(File::with_name("config").format(FileFormat::Toml).required(false))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}