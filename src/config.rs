use serde::{self, Deserialize, Serialize};
use std::default::Default;
use toml;

#[derive(Deserialize, Serialize)]
pub struct Config {
    ip: String,
    port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ip: "192.168.64.1".to_owned(),
            port: 9901,
        }
    }
}

pub fn from_file() -> *const Config {
    let c: Config = Default::default();
    &c
}
pub fn to_file(config: &Config) {}
