//! App-context enums + tiny env-var utility surface. Std-only; the harness
//! stays zero-dep. `parse` is exposed alongside `from_env` so tests can drive
//! the matching logic directly without mutating process env.

use std::env;

/// Deployment environment - LOCAL by default if `APP_ENV` is unset, empty, or
/// holds an unrecognised value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SubMsAppEnv {
    #[default]
    Local,
    Dev,
    Uat,
    Prod,
}

impl SubMsAppEnv {
    pub fn from_env() -> Self {
        env_str("APP_ENV")
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "local" => Self::Local,
            "dev" | "development" => Self::Dev,
            "uat" | "staging" | "stage" => Self::Uat,
            "prod" | "production" => Self::Prod,
            _ => Self::Local,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Uat => "uat",
            Self::Prod => "prod",
        }
    }
}

impl std::fmt::Display for SubMsAppEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Deployment region - UNKNOWN by default if `APP_REGION` is unset, empty,
/// or holds an unrecognised value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SubMsAppRegion {
    Na,
    Latam,
    Emea,
    Apac,
    #[default]
    Unknown,
}

impl SubMsAppRegion {
    pub fn from_env() -> Self {
        env_str("APP_REGION")
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "na" | "north-america" | "north_america" | "namer" => Self::Na,
            "latam" | "lat-am" | "latin-america" | "latin_america" => Self::Latam,
            "emea" => Self::Emea,
            "apac" | "asia-pacific" | "asia_pacific" => Self::Apac,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Na => "na",
            Self::Latam => "latam",
            Self::Emea => "emea",
            Self::Apac => "apac",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for SubMsAppRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Returns None for unset OR empty env vars; both shapes are treated as absent.
pub fn env_str(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}

pub fn env_or(key: &str, default: impl Into<String>) -> String {
    env_str(key).unwrap_or_else(|| default.into())
}

pub fn env_bool(key: &str, default: bool) -> bool {
    match env_str(key) {
        None => default,
        Some(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => default,
        },
    }
}

pub fn env_i64(key: &str, default: i64) -> i64 {
    env_str(key)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

pub fn env_u64(key: &str, default: u64) -> u64 {
    env_str(key)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

pub fn env_f64(key: &str, default: f64) -> f64 {
    env_str(key)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}
