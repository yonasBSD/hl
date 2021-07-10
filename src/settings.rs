// std imports
use std::collections::HashMap;
use std::include_str;
use std::str;

// third-party imports
use chrono_tz::Tz;
use config::{Config, File, FileFormat};
use derive_deref::Deref;
use platform_dirs::AppDirs;
use serde::Deserialize;

// local imports
use crate::error::Error;
use crate::types::Level;

// ---

static DEFAULT_SETTINGS: &str = include_str!("../etc/defaults/config.yaml");

// ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    pub fields: Fields,
    pub concurrency: Option<usize>,
    pub time_format: String,
    pub time_zone: Tz,
}

impl Settings {
    pub fn load(app_dirs: &AppDirs) -> Result<Self, Error> {
        let mut s = Config::default();
        let filename = app_dirs.config_dir.join("config.yaml");

        s.merge(File::from_str(DEFAULT_SETTINGS, FileFormat::Yaml))?;
        s.merge(File::with_name(&filename.to_string_lossy()).required(false))?;

        Ok(s.try_into()?)
    }
}

// ---

#[derive(Debug, Deserialize)]
pub struct Fields {
    pub time: TimeField,
    pub level: LevelField,
    pub message: MessageField,
    pub logger: LoggerField,
    pub caller: CallerField,
}

// ---

#[derive(Debug, Deserialize, Deref)]
pub struct TimeField(pub Field);

// ---

#[derive(Debug, Deserialize)]
pub struct LevelField {
    pub variants: Vec<LevelFieldVariant>,
}

// ---

#[derive(Debug, Deserialize)]
pub struct LevelFieldVariant {
    pub names: Vec<String>,
    pub values: HashMap<Level, Vec<String>>,
}

// ---

#[derive(Debug, Deserialize, Deref)]
pub struct MessageField(Field);

// ---

#[derive(Debug, Deserialize, Deref)]
pub struct LoggerField(Field);

// ---

#[derive(Debug, Deserialize, Deref)]
pub struct CallerField(Field);

// ---

#[derive(Debug, Deserialize)]
pub struct Field {
    pub names: Vec<String>,
}
