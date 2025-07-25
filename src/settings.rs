// std imports
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    include_str,
    ops::Deref,
    path::{Path, PathBuf},
    str::FromStr,
};

// third-party imports
use chrono_tz::Tz;
use config::{Config, File, FileFormat};
use derive_more::{Deref, From};
use enumset::{EnumSet, EnumSetType, enum_set};
use enumset_ext::EnumSetExt;
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::IntoDeserializer};
use strum::{Display, IntoEnumIterator};

// local imports
use crate::level::{InfallibleLevel, Level};
use crate::{error::Error, xerr::Suggestions};

// sub-modules
pub mod error;

// ---

static DEFAULT_SETTINGS_RAW: &str = include_str!("../etc/defaults/config.yaml");
static DEFAULT_SETTINGS: Lazy<Settings> = Lazy::new(|| Settings::load([Source::string("", FileFormat::Yaml)]).unwrap());

// ---

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    pub fields: Fields,
    pub concurrency: Option<usize>,
    pub time_format: String,
    pub time_zone: Tz,
    pub formatting: Formatting,
    pub theme: String,
    #[serde(deserialize_with = "enumset_serde::deserialize")]
    pub input_info: InputInfoSet,
}

impl Settings {
    pub fn load<I>(sources: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = Source>,
    {
        let mut builder = Config::builder().add_source(File::from_str(DEFAULT_SETTINGS_RAW, FileFormat::Yaml));

        for source in sources {
            builder = match source {
                Source::File(SourceFile { filename, required }) => {
                    log::debug!(
                        "added configuration file {} search path: {}",
                        if required { "required" } else { "optional" },
                        filename.display(),
                    );
                    builder.add_source(File::from(filename.as_path()).required(required))
                }
                Source::String(value, format) => builder.add_source(File::from_str(&value, format)),
            };
        }

        Ok(builder.build()?.try_deserialize()?)
    }
}

impl Default for Settings {
    fn default() -> Self {
        DEFAULT_SETTINGS.clone()
    }
}

impl Default for &'static Settings {
    fn default() -> Self {
        &DEFAULT_SETTINGS
    }
}

// ---

pub enum Source {
    File(SourceFile),
    String(String, FileFormat),
}

impl Source {
    pub fn string<S>(value: S, format: FileFormat) -> Self
    where
        S: Into<String>,
    {
        Self::String(value.into(), format)
    }
}

impl From<SourceFile> for Source {
    fn from(file: SourceFile) -> Self {
        Self::File(file)
    }
}

// ---

pub struct SourceFile {
    filename: PathBuf,
    required: bool,
}

impl SourceFile {
    pub fn new<P>(filename: P) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            filename: filename.as_ref().into(),
            required: true,
        }
    }

    pub fn required(self, required: bool) -> Self {
        Self { required, ..self }
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub predefined: PredefinedFields,
    pub ignore: Vec<String>,
    pub hide: Vec<String>,
}

// ---

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PredefinedFields {
    pub time: TimeField,
    pub level: LevelField,
    pub message: MessageField,
    pub logger: LoggerField,
    pub caller: CallerField,
    pub caller_file: CallerFileField,
    pub caller_line: CallerLineField,
}

impl Default for &PredefinedFields {
    fn default() -> Self {
        static DEFAULT: Lazy<PredefinedFields> = Lazy::new(|| PredefinedFields::default());
        &DEFAULT
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct TimeField(pub Field);

impl Default for TimeField {
    fn default() -> Self {
        Self(Field::new(vec!["time".into(), "ts".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LevelField {
    pub show: FieldShowOption,
    pub variants: Vec<RawLevelFieldVariant>,
}

impl Default for LevelField {
    fn default() -> Self {
        Self {
            show: FieldShowOption::default(),
            variants: vec![RawLevelFieldVariant {
                names: vec!["level".into()],
                values: Level::iter()
                    .map(|level| (level.into(), vec![level.as_ref().to_lowercase().into()]))
                    .collect(),
                level: None,
            }],
        }
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RawLevelFieldVariant {
    pub names: Vec<String>,
    #[serde(default, serialize_with = "ordered_map_serialize")]
    pub values: HashMap<InfallibleLevel, Vec<String>>,
    pub level: Option<InfallibleLevel>,
}

impl RawLevelFieldVariant {
    pub fn resolve(&self) -> Option<LevelFieldVariant> {
        let mut unknowns = HashSet::new();
        let mut values = HashMap::new();
        let mut valid = true;

        for (level, names) in &self.values {
            match level {
                InfallibleLevel::Valid(level) => {
                    values.insert(level.clone(), names.clone());
                }
                InfallibleLevel::Invalid(name) => {
                    unknowns.insert(name.clone());
                }
            }
        }

        let level = self.level.clone().and_then(|level| match level {
            InfallibleLevel::Valid(level) => Some(level),
            InfallibleLevel::Invalid(name) => {
                unknowns.insert(name);
                valid = false;
                None
            }
        });

        for name in unknowns {
            log::warn!("unknown level: {:?}", name);
        }

        if valid && !values.is_empty() {
            Some(LevelFieldVariant {
                names: self.names.clone(),
                values,
                level,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LevelFieldVariant {
    pub names: Vec<String>,
    #[serde(default, serialize_with = "ordered_map_serialize")]
    pub values: HashMap<Level, Vec<String>>,
    pub level: Option<Level>,
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct MessageField(pub Field);

impl Default for MessageField {
    fn default() -> Self {
        Self(Field::new(vec!["msg".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct LoggerField(Field);

impl Default for LoggerField {
    fn default() -> Self {
        Self(Field::new(vec!["logger".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct CallerField(Field);

impl Default for CallerField {
    fn default() -> Self {
        Self(Field::new(vec!["caller".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct CallerFileField(Field);

impl Default for CallerFileField {
    fn default() -> Self {
        Self(Field::new(vec!["file".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Deref, Clone, PartialEq, Eq, From)]
pub struct CallerLineField(Field);

impl Default for CallerLineField {
    fn default() -> Self {
        Self(Field::new(vec!["line".into()]))
    }
}

// ---

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Field {
    pub names: Vec<String>,
    #[serde(default)]
    pub show: FieldShowOption,
}

impl Field {
    pub fn new(names: Vec<String>) -> Self {
        Self {
            names,
            show: FieldShowOption::Auto,
        }
    }
}

// ---

pub type InputInfoSet = EnumSet<InputInfo>;

#[derive(Debug, Serialize, Deserialize, EnumSetType, Display)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum InputInfo {
    Auto,
    None,
    Minimal,
    Compact,
    Full,
}

impl InputInfo {
    pub fn resolve(set: enumset::EnumSet<InputInfo>) -> enumset::EnumSet<InputInfo> {
        if !set.intersects(enum_set!(InputInfo::Auto).complement()) {
            enumset::EnumSet::all()
        } else {
            set
        }
        .difference(InputInfo::Auto.into())
    }
}

impl FromStr for InputInfo {
    type Err = self::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_plain::from_str(s).map_err(|_| Self::Err::InvalidInputInfo {
            value: s.into(),
            suggestions: Suggestions::new(s, InputInfoSet::all().iter().map(|v| v.to_string())),
        })
    }
}

// ---

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Formatting {
    pub flatten: Option<FlattenOption>,
    pub message: MessageFormatting,
    pub punctuation: Punctuation,
}

// ---

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct MessageFormatting {
    pub format: MessageFormat,
}

// ---

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum MessageFormat {
    AutoQuoted,
    AlwaysQuoted,
    AlwaysDoubleQuoted,
    #[default]
    Delimited,
    Raw,
}

// ---

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FlattenOption {
    Never,
    Always,
}

// ---

#[derive(Debug, Serialize, Deserialize, Default, Clone, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum FieldShowOption {
    #[default]
    Auto,
    Always,
}

// ---

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Punctuation {
    pub logger_name_separator: String,
    pub field_key_value_separator: String,
    pub string_opening_quote: String,
    pub string_closing_quote: String,
    pub source_location_separator: String,
    pub caller_name_file_separator: String,
    pub hidden_fields_indicator: String,
    pub level_left_separator: String,
    pub level_right_separator: String,
    pub input_number_prefix: String,
    pub input_number_left_separator: String,
    pub input_number_right_separator: String,
    pub input_name_left_separator: String,
    pub input_name_right_separator: String,
    pub input_name_clipping: String,
    pub input_name_common_part: String,
    pub array_separator: String,
    pub message_delimiter: String,
}

impl Default for Punctuation {
    fn default() -> Self {
        Self {
            logger_name_separator: ":".into(),
            field_key_value_separator: "=".into(),
            string_opening_quote: "'".into(),
            string_closing_quote: "'".into(),
            source_location_separator: "@ ".into(),
            caller_name_file_separator: " ".into(),
            hidden_fields_indicator: " ...".into(),
            level_left_separator: "|".into(),
            level_right_separator: "|".into(),
            input_number_prefix: "#".into(),
            input_number_left_separator: "".into(),
            input_number_right_separator: " | ".into(),
            input_name_left_separator: "".into(),
            input_name_right_separator: " | ".into(),
            input_name_clipping: "...".into(),
            input_name_common_part: "...".into(),
            array_separator: " ".into(),
            message_delimiter: "::".into(),
        }
    }
}

impl Punctuation {
    #[cfg(test)]
    pub fn test_default() -> Self {
        Self {
            logger_name_separator: ":".into(),
            field_key_value_separator: "=".into(),
            string_opening_quote: "'".into(),
            string_closing_quote: "'".into(),
            source_location_separator: "@ ".into(),
            caller_name_file_separator: " :: ".into(),
            hidden_fields_indicator: " ...".into(),
            level_left_separator: "|".into(),
            level_right_separator: "|".into(),
            input_number_prefix: "#".into(),
            input_number_left_separator: "".into(),
            input_number_right_separator: " | ".into(),
            input_name_left_separator: "".into(),
            input_name_right_separator: " | ".into(),
            input_name_clipping: "...".into(),
            input_name_common_part: "...".into(),
            array_separator: ",".into(),
            message_delimiter: "::".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ListOrCommaSeparatedList<T> {
    List(Vec<T>),
    #[serde(deserialize_with = "csl_deserialize")]
    CommaSeparatedList(Vec<T>),
}

impl<T> Deref for ListOrCommaSeparatedList<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        match self {
            ListOrCommaSeparatedList::List(list) => list,
            ListOrCommaSeparatedList::CommaSeparatedList(list) => list,
        }
    }
}

impl<T> From<ListOrCommaSeparatedList<T>> for Vec<T> {
    fn from(value: ListOrCommaSeparatedList<T>) -> Self {
        match value {
            ListOrCommaSeparatedList::List(list) => list,
            ListOrCommaSeparatedList::CommaSeparatedList(list) => list,
        }
    }
}

impl<T> From<Vec<T>> for ListOrCommaSeparatedList<T> {
    fn from(value: Vec<T>) -> Self {
        ListOrCommaSeparatedList::List(value)
    }
}

fn ordered_map_serialize<K: Eq + PartialEq + Ord + PartialOrd + Serialize, V: Serialize, S>(
    value: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let ordered: BTreeMap<_, _> = value.iter().collect();
    ordered.serialize(serializer)
}

fn csl_deserialize<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let seq = String::deserialize(deserializer)?;
    if seq.is_empty() {
        Ok(Vec::new())
    } else {
        seq.split(',')
            .map(|item| T::deserialize(item.trim().into_deserializer()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let test = |settings: &Settings| {
            assert_eq!(settings.concurrency, None);
            assert_eq!(settings.time_format, "%b %d %T.%3N");
            assert_eq!(settings.time_zone, chrono_tz::UTC);
            assert_eq!(settings.theme, "uni");
        };

        let settings: &'static Settings = Default::default();
        test(settings);
        test(&Settings::default());
    }

    #[test]
    fn test_load_settings_k8s() {
        let settings = Settings::load([SourceFile::new("etc/defaults/config-k8s.yaml").into()]).unwrap();
        assert_eq!(
            settings.fields.predefined.time,
            TimeField(Field {
                names: vec!["ts".into()],
                show: FieldShowOption::Auto,
            })
        );
        assert_eq!(settings.time_format, "%b %d %T.%3N");
        assert_eq!(settings.time_zone, chrono_tz::UTC);
        assert_eq!(settings.theme, "uni");
    }

    #[test]
    fn test_unknown_level_values() {
        let variant = RawLevelFieldVariant {
            names: vec!["level".into()],
            values: vec![
                (InfallibleLevel::Valid(Level::Info), vec!["info".into()]),
                (InfallibleLevel::Invalid("unknown".into()), vec!["unknown".into()]),
            ]
            .into_iter()
            .collect(),
            level: None,
        };

        assert_eq!(
            variant.resolve(),
            Some(LevelFieldVariant {
                names: vec!["level".into()],
                values: vec![(Level::Info, vec!["info".into()])].into_iter().collect(),
                level: None,
            })
        );
    }

    #[test]
    fn test_unknown_level_main() {
        let variant = RawLevelFieldVariant {
            names: vec!["level".into()],
            values: vec![(InfallibleLevel::Valid(Level::Info), vec!["info".into()])]
                .into_iter()
                .collect(),
            level: Some(InfallibleLevel::Invalid("unknown".into())),
        };

        assert_eq!(variant.resolve(), None);
    }

    #[test]
    fn test_unknown_level_all_unknown() {
        let variant = RawLevelFieldVariant {
            names: vec!["level".into()],
            values: vec![(InfallibleLevel::Invalid("unknown".into()), vec!["unknown".into()])]
                .into_iter()
                .collect(),
            level: Some(InfallibleLevel::Valid(Level::Info)),
        };

        assert_eq!(variant.resolve(), None);
    }

    #[test]
    fn test_csl() {
        let csl = ListOrCommaSeparatedList::from(vec!["a", "b", "c"]);
        assert_eq!(csl.deref(), vec!["a", "b", "c"]);
        assert_eq!(Vec::from(csl), vec!["a", "b", "c"]);

        let csl: ListOrCommaSeparatedList<String> = serde_plain::from_str("a,b,c").unwrap();
        assert_eq!(csl.deref(), vec!["a", "b", "c"]);
        assert_eq!(Vec::from(csl), vec!["a", "b", "c"]);

        let csl: ListOrCommaSeparatedList<String> = serde_plain::from_str("").unwrap();
        assert_eq!(csl.deref(), Vec::<String>::new());

        let csl = serde_json::from_str::<ListOrCommaSeparatedList<String>>(r#""a,b,c""#).unwrap();
        assert_eq!(csl.deref(), vec!["a", "b", "c"]);

        let csl = serde_json::from_str::<ListOrCommaSeparatedList<String>>(r#""""#).unwrap();
        assert_eq!(csl.deref(), Vec::<String>::new());

        let res = serde_json::from_str::<ListOrCommaSeparatedList<String>>(r#"12"#);
        assert!(res.is_err());
    }
}
