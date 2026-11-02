//! Publication Channels

use serde::{Deserialize, Serialize};

/// Publication channel
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationChannel {
    Cli,
    Tui,
    Web,
    Report,
    Markdown,
    PressRelease,
    StatusUpdate,
    IncidentReport,
    ExecutiveBrief,
    AcademicReport,
    Api,
    Custom(String),
}

/// Channel identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelId(pub String);

impl ChannelId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl PublicationChannel {
    pub fn default_format(&self) -> &'static str {
        match self {
            Self::Cli => "text",
            Self::Tui => "ansi",
            Self::Web => "html",
            Self::Report => "markdown",
            Self::Markdown => "markdown",
            Self::PressRelease => "markdown",
            Self::StatusUpdate => "text",
            Self::IncidentReport => "markdown",
            Self::ExecutiveBrief => "markdown",
            Self::AcademicReport => "markdown",
            Self::Api => "json",
            Self::Custom(_) => "markdown",
        }
    }
}

impl Default for PublicationChannel {
    fn default() -> Self {
        Self::Report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_formats() {
        assert_eq!(PublicationChannel::Cli.default_format(), "text");
        assert_eq!(PublicationChannel::Web.default_format(), "html");
        assert_eq!(PublicationChannel::Report.default_format(), "markdown");
        assert_eq!(PublicationChannel::Api.default_format(), "json");
    }
}