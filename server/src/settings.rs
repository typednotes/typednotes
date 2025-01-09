use config::{Config, ConfigError, Environment, File, FileFormat};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Database {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: String,
    pub database: String,
}

impl Database {
    pub fn url(&self) -> String {
        format!("postgres://{}:{}@{}:{}/{}", self.user, self.password, self.host, self.port, self.database)
    }
}

impl Default for Database {
    fn default() -> Self {
        Self {
            user: "typednotes".into(),
            password: "password".into(),
            host: "localhost".into(),
            port: "5432".into(),
            database: "typednotes".into(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[allow(unused)]
pub struct Auth {
    pub redirect_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[allow(unused)]
pub struct GitHub {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Deserialize, Default)]
#[allow(unused)]
pub struct Settings {
    pub database: Database,
    pub auth: Auth,
    pub github: GitHub,
}

impl Settings {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name("config.toml").format(FileFormat::Toml).required(false))
            .add_source(Environment::default().separator("_").ignore_empty(true))
            .build()?;

        // You can deserialize (and thus freeze) the entire configuration as
        config.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use super::*;

    #[test]
    fn test_settings() {
        set_var("DATABASE_USER", "test_user");
        let settings = Settings::new().unwrap_or_default();
        println!("Settings = {:?}", settings);
        assert_eq!(settings.database.url(), "postgres://test_user:password@localhost:5432/typednotes");
    }
}