use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use vantadeck_domain::AppCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub category: AppCategory,
    pub platforms: Vec<Platform>,
    pub executables: Vec<String>,
    #[serde(default)]
    pub known_paths: Vec<String>,
    #[serde(default)]
    pub file_types: Vec<String>,
    #[serde(default)]
    pub launch_templates: Vec<LaunchTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewState {
    Submitted,
    Reviewed,
    Verified,
    Stale,
    Withdrawn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolArtifact {
    pub platform: Platform,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_url: String,
    pub license: String,
    pub supported_hosts: Vec<String>,
    pub platforms: Vec<Platform>,
    pub provenance: String,
    pub review_state: ReviewState,
    pub last_verified_at: String,
    pub safety_notes: String,
    pub artifacts: Vec<ToolArtifact>,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest JSON is invalid: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("manifest schema version {0} is unsupported")]
    UnsupportedSchema(u32),
    #[error("manifest field `{0}` cannot be empty")]
    EmptyField(&'static str),
    #[error("launch template contains unsafe argument: {argument}")]
    UnsafeArgument { argument: String },
    #[error("manifest URL must use HTTPS: {url}")]
    UnsafeUrl { url: String },
    #[error("artifact SHA-256 digest is invalid: {digest}")]
    InvalidChecksum { digest: String },
    #[error("manifest verification date must use YYYY-MM-DD: {date}")]
    InvalidDate { date: String },
}

impl AppManifest {
    pub fn from_json(input: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(input)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::UnsupportedSchema(self.schema_version));
        }
        if self.id.trim().is_empty() {
            return Err(ManifestError::EmptyField("id"));
        }
        if self.executables.is_empty() {
            return Err(ManifestError::EmptyField("executables"));
        }
        for argument in self
            .launch_templates
            .iter()
            .flat_map(|template| &template.arguments)
        {
            if contains_shell_syntax(argument) {
                return Err(ManifestError::UnsafeArgument {
                    argument: argument.clone(),
                });
            }
        }
        Ok(())
    }
}

impl ToolManifest {
    pub fn from_json(input: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(input)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::UnsupportedSchema(self.schema_version));
        }
        for (field, value) in [
            ("id", self.id.as_str()),
            ("name", self.name.as_str()),
            ("description", self.description.as_str()),
            ("license", self.license.as_str()),
            ("provenance", self.provenance.as_str()),
            ("safetyNotes", self.safety_notes.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ManifestError::EmptyField(field));
            }
        }
        if !valid_id(&self.id) {
            return Err(ManifestError::EmptyField("id"));
        }
        if self.platforms.is_empty() {
            return Err(ManifestError::EmptyField("platforms"));
        }
        if has_duplicates(&self.supported_hosts) {
            return Err(ManifestError::EmptyField("supportedHosts must be unique"));
        }
        if self
            .platforms
            .iter()
            .enumerate()
            .any(|(index, platform)| self.platforms[..index].contains(platform))
        {
            return Err(ManifestError::EmptyField("platforms must be unique"));
        }
        validate_https(&self.source_url)?;
        if !is_iso_date(&self.last_verified_at) {
            return Err(ManifestError::InvalidDate {
                date: self.last_verified_at.clone(),
            });
        }
        for artifact in &self.artifacts {
            validate_https(&artifact.url)?;
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ManifestError::InvalidChecksum {
                    digest: artifact.sha256.clone(),
                });
            }
        }
        Ok(())
    }
}

fn validate_https(url: &str) -> Result<(), ManifestError> {
    let authority = url
        .strip_prefix("https://")
        .and_then(|remainder| remainder.split(['/', '?', '#']).next());
    if authority.is_some_and(valid_authority)
        && !url
            .chars()
            .any(|character| character.is_whitespace() || character == '\\')
    {
        Ok(())
    } else {
        Err(ManifestError::UnsafeUrl { url: url.into() })
    }
}

fn valid_authority(authority: &str) -> bool {
    let host_and_port = match authority.split_once('@') {
        Some((userinfo, host)) if valid_userinfo(userinfo) && !host.contains('@') => host,
        Some(_) => return false,
        None => authority,
    };
    if host_and_port.is_empty() {
        return false;
    }
    if let Some(ipv6) = host_and_port.strip_prefix('[') {
        let Some((host, suffix)) = ipv6.split_once(']') else {
            return false;
        };
        return host.parse::<Ipv6Addr>().is_ok()
            && (suffix.is_empty() || suffix.strip_prefix(':').is_some_and(valid_port));
    }
    if host_and_port.contains(['[', ']']) {
        return false;
    }
    let (host, port) = host_and_port
        .rsplit_once(':')
        .map_or((host_and_port, None), |(host, port)| (host, Some(port)));
    valid_host(host) && port.is_none_or(valid_port)
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok()
}

fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || host.contains(':') {
        return false;
    }
    if host.parse::<Ipv4Addr>().is_ok() {
        return true;
    }
    if host
        .split('.')
        .all(|label| !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    host.split('.').all(|label| {
        label.len() <= 63
            && label
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && label
                .bytes()
                .last()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn valid_userinfo(userinfo: &str) -> bool {
    let bytes = userinfo.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
            )
        {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

fn has_duplicates(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit()))
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
        && !value.ends_with('-')
}

fn contains_shell_syntax(value: &str) -> bool {
    ["&&", "||", ";", "`", "$(", "\n", "\r", ">", "<", "|"]
        .iter()
        .any(|token| value.contains(token))
}
