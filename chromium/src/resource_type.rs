// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use std::str::FromStr;

/// Blink resource type for one Chromium request.
///
/// [`Self::adblock_type`] returns the string Brave passes into adblock-rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChromiumResourceType {
    MainFrame,
    SubFrame,
    Stylesheet,
    Script,
    Image,
    Favicon,
    Font,
    SubResource,
    Object,
    Media,
    Xhr,
    Ping,
    Worker,
    SharedWorker,
    ServiceWorker,
    Prefetch,
    CspReport,
    Plugin,
    NavigationPreload,
    Other,
}

impl ChromiumResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MainFrame => "main_frame",
            Self::SubFrame => "sub_frame",
            Self::Stylesheet => "stylesheet",
            Self::Script => "script",
            Self::Image => "image",
            Self::Favicon => "favicon",
            Self::Font => "font",
            Self::SubResource => "sub_resource",
            Self::Object => "object",
            Self::Media => "media",
            Self::Xhr => "xhr",
            Self::Ping => "ping",
            Self::Worker => "worker",
            Self::SharedWorker => "shared_worker",
            Self::ServiceWorker => "service_worker",
            Self::Prefetch => "prefetch",
            Self::CspReport => "csp_report",
            Self::Plugin => "plugin",
            Self::NavigationPreload => "navigation_preload",
            Self::Other => "other",
        }
    }

    /// Type string consumed by `adblock::request::Request`.
    ///
    /// Favicons are images. Workers, prefetch, CSP reports, plugins, and
    /// navigation preload use an empty string, which the engine treats as
    /// `other`. That is Brave's map, including the empty fallback.
    pub fn adblock_type(self) -> &'static str {
        match self {
            Self::MainFrame => "main_frame",
            Self::SubFrame => "sub_frame",
            Self::Stylesheet => "stylesheet",
            Self::Script => "script",
            Self::Image | Self::Favicon => "image",
            Self::Font => "font",
            Self::SubResource | Self::Other => "other",
            Self::Object => "object",
            Self::Media => "media",
            Self::Xhr => "xhr",
            Self::Ping => "ping",
            Self::Worker
            | Self::SharedWorker
            | Self::ServiceWorker
            | Self::Prefetch
            | Self::CspReport
            | Self::Plugin
            | Self::NavigationPreload => "",
        }
    }

    pub fn is_document_navigation(self, url: &str) -> bool {
        matches!(self, Self::MainFrame) && is_http_scheme(url)
    }
}

pub fn is_supported_scheme(url: &str) -> bool {
    let Some(scheme) = url_scheme(url) else {
        return false;
    };
    matches!(scheme.as_str(), "http" | "https" | "ws" | "wss")
}

pub fn is_http_scheme(url: &str) -> bool {
    let Some(scheme) = url_scheme(url) else {
        return false;
    };
    matches!(scheme.as_str(), "http" | "https")
}

fn url_scheme(url: &str) -> Option<String> {
    let (scheme, _) = url.split_once(':')?;
    Some(scheme.to_ascii_lowercase())
}

impl fmt::Display for ChromiumResourceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownResourceType {
    value: String,
}

impl UnknownResourceType {
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for UnknownResourceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown Chromium resource type `{}`", self.value)
    }
}

impl std::error::Error for UnknownResourceType {}

impl FromStr for ChromiumResourceType {
    type Err = UnknownResourceType;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        let parsed = match normalized.as_str() {
            "main_frame" => Self::MainFrame,
            "sub_frame" => Self::SubFrame,
            "stylesheet" => Self::Stylesheet,
            "script" => Self::Script,
            "image" => Self::Image,
            "favicon" => Self::Favicon,
            "font" => Self::Font,
            "sub_resource" => Self::SubResource,
            "object" => Self::Object,
            "media" => Self::Media,
            "xhr" => Self::Xhr,
            "ping" => Self::Ping,
            "worker" => Self::Worker,
            "shared_worker" => Self::SharedWorker,
            "service_worker" => Self::ServiceWorker,
            "prefetch" => Self::Prefetch,
            "csp_report" => Self::CspReport,
            "plugin" => Self::Plugin,
            "navigation_preload" => Self::NavigationPreload,
            "other" => Self::Other,
            _ => {
                return Err(UnknownResourceType {
                    value: value.to_string(),
                });
            }
        };
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::ChromiumResourceType;

    #[test]
    fn names_round_trip() {
        let types = [
            ChromiumResourceType::MainFrame,
            ChromiumResourceType::SubFrame,
            ChromiumResourceType::Stylesheet,
            ChromiumResourceType::Script,
            ChromiumResourceType::Image,
            ChromiumResourceType::Favicon,
            ChromiumResourceType::Font,
            ChromiumResourceType::SubResource,
            ChromiumResourceType::Object,
            ChromiumResourceType::Media,
            ChromiumResourceType::Xhr,
            ChromiumResourceType::Ping,
            ChromiumResourceType::Worker,
            ChromiumResourceType::SharedWorker,
            ChromiumResourceType::ServiceWorker,
            ChromiumResourceType::Prefetch,
            ChromiumResourceType::CspReport,
            ChromiumResourceType::Plugin,
            ChromiumResourceType::NavigationPreload,
            ChromiumResourceType::Other,
        ];
        for resource_type in types {
            let parsed: ChromiumResourceType = resource_type.as_str().parse().unwrap();
            assert_eq!(parsed, resource_type);
        }
    }

    #[test]
    fn hyphenated_names_parse() {
        let parsed: ChromiumResourceType = "main-frame".parse().unwrap();
        assert_eq!(parsed, ChromiumResourceType::MainFrame);
    }

    #[test]
    fn brave_type_map() {
        assert_eq!(ChromiumResourceType::Favicon.adblock_type(), "image");
        assert_eq!(ChromiumResourceType::SubResource.adblock_type(), "other");
        assert_eq!(ChromiumResourceType::Worker.adblock_type(), "");
        assert_eq!(ChromiumResourceType::Plugin.adblock_type(), "");
        assert_eq!(ChromiumResourceType::Script.adblock_type(), "script");
        assert!(ChromiumResourceType::MainFrame.is_document_navigation("https://news.example/"));
        assert!(
            !ChromiumResourceType::MainFrame.is_document_navigation("wss://news.example/socket")
        );
        assert!(!ChromiumResourceType::SubFrame.is_document_navigation("https://news.example/"));
    }
}
