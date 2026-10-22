// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::path::Path;

use adblock::blocker::BlockerResult;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::resources::Resource;
use adblock::Engine;

use crate::cosmetic::CosmeticPlan;
use crate::data_url::parse_data_url;
use crate::decision::{MatchInfo, NetworkDecision};
use crate::lists::{read_filter_lists, ListError};
use crate::request::ChromiumResourceRequest;
use crate::resource_type::{is_http_scheme, is_supported_scheme, ChromiumResourceType};

/// Brave's adblock engine applied to Chromium resource requests.
///
/// The value is `Send + Sync`. Share it across Chromium's IO and UI threads.
/// Compiling new lists replaces the host; a live engine is not edited in place.
pub struct BraveAdblockHost {
    engine: Engine,
}

#[derive(Debug)]
pub enum HostError {
    List(ListError),
    Dat(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List(error) => error.fmt(formatter),
            Self::Dat(message) => {
                write!(formatter, "failed to read compiled filter cache: {message}")
            }
        }
    }
}

impl std::error::Error for HostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::List(error) => Some(error),
            Self::Dat(_) => None,
        }
    }
}

impl BraveAdblockHost {
    pub fn compile(
        lists: impl IntoIterator<Item = impl Into<String>>,
        resources: Vec<Resource>,
    ) -> Self {
        let mut filter_set = FilterSet::new(true);
        for list in lists {
            let text = list.into();
            if text.trim().is_empty() {
                continue;
            }
            filter_set.add_filter_list(text, ParseOptions::default());
        }
        Self::from_filter_set(filter_set, resources)
    }

    pub fn from_files<P: AsRef<Path>>(
        paths: &[P],
        resources: Vec<Resource>,
    ) -> Result<Self, HostError> {
        let lists = read_filter_lists(paths).map_err(HostError::List)?;
        Ok(Self::compile(lists, resources))
    }

    /// Compiled rules only. Redirect and scriptlet resources are not in the cache.
    pub fn serialize(&self) -> Vec<u8> {
        self.engine.serialize()
    }

    pub fn from_dat(bytes: &[u8], resources: Vec<Resource>) -> Result<Self, HostError> {
        let mut engine = Engine::new_with_filter_set(FilterSet::new(true));
        engine
            .deserialize(bytes)
            .map_err(|error| HostError::Dat(error.to_string()))?;
        install_resources(&mut engine, resources);
        Ok(Self { engine })
    }

    #[must_use]
    pub fn decide(&self, request: &ChromiumResourceRequest) -> NetworkDecision {
        self.decide_with_canonical_host(request, None)
    }

    /// `canonical_host` is a hostname Chromium already resolved. This host does
    /// not perform DNS. A `$removeparam` rewrite from the canonical URL is
    /// dropped; a rewrite from the original URL is kept when the canonical
    /// check does not block.
    #[must_use]
    pub fn decide_with_canonical_host(
        &self,
        request: &ChromiumResourceRequest,
        canonical_host: Option<&str>,
    ) -> NetworkDecision {
        if !is_supported_scheme(&request.url) {
            return NetworkDecision::allow();
        }
        let original = self.evaluate(request);
        if original.blocked() {
            return original;
        }
        let Some(canonical_host) = canonical_host
            .map(str::trim)
            .filter(|host| !host.is_empty())
        else {
            return original;
        };
        if hosts_equal(&request.url, canonical_host) {
            return original;
        }
        let Some(rewritten) = replace_host(&request.url, canonical_host) else {
            return original;
        };
        let mut cloaked = request.clone();
        cloaked.url = rewritten;
        let canonical = self.evaluate(&cloaked);
        if canonical.blocked() {
            return canonical;
        }
        original
    }

    #[must_use]
    pub fn content_security_policy(&self, request: &ChromiumResourceRequest) -> Option<String> {
        if !matches!(
            request.resource_type,
            ChromiumResourceType::MainFrame | ChromiumResourceType::SubFrame
        ) {
            return None;
        }
        if !is_http_scheme(&request.url) {
            return None;
        }
        let source = request.initiator.as_deref().unwrap_or("");
        let parsed = Request::new(
            &request.url,
            source,
            request.resource_type.adblock_type(),
            &request.method,
        )
        .ok()?;
        let directives = self.engine.get_csp_directives(&parsed)?;
        if directives.is_empty() {
            None
        } else {
            Some(directives)
        }
    }

    #[must_use]
    pub fn cosmetics(&self, url: &str) -> CosmeticPlan {
        self.cosmetics_for_dom(url, &[] as &[&str], &[] as &[&str])
    }

    /// `classes` and `ids` are bare DOM tokens (`banner`, not `.banner`).
    #[must_use]
    pub fn cosmetics_for_dom<C, I>(&self, url: &str, classes: &[C], ids: &[I]) -> CosmeticPlan
    where
        C: AsRef<str>,
        I: AsRef<str>,
    {
        let resources = self.engine.url_cosmetic_resources(url);
        let generic = if resources.generichide || (classes.is_empty() && ids.is_empty()) {
            Vec::new()
        } else {
            self.engine.hidden_class_id_selectors(
                classes.iter().map(AsRef::as_ref),
                ids.iter().map(AsRef::as_ref),
                &resources.exceptions,
            )
        };
        CosmeticPlan::from_resources(resources, generic)
    }

    fn evaluate(&self, request: &ChromiumResourceRequest) -> NetworkDecision {
        if !is_supported_scheme(&request.url) {
            return NetworkDecision::allow();
        }
        let source = request.initiator.as_deref().unwrap_or("");
        let Ok(parsed) = Request::new(
            &request.url,
            source,
            request.resource_type.adblock_type(),
            &request.method,
        ) else {
            return NetworkDecision::allow();
        };
        let result = self.engine.check_network_request(&parsed);
        decision_from(request, &result)
    }

    fn from_filter_set(filter_set: FilterSet, resources: Vec<Resource>) -> Self {
        let mut engine = Engine::new_with_filter_set(filter_set);
        install_resources(&mut engine, resources);
        Self { engine }
    }
}

fn install_resources(engine: &mut Engine, resources: Vec<Resource>) {
    if !resources.is_empty() {
        engine.use_resources(resources);
    }
}

fn decision_from(request: &ChromiumResourceRequest, result: &BlockerResult) -> NetworkDecision {
    let info = MatchInfo {
        rule: result
            .filter
            .as_ref()
            .and_then(|filter| filter.raw_line.clone()),
        exception: result
            .exception
            .as_ref()
            .and_then(|filter| filter.raw_line.clone()),
        important: result.important,
    };
    if result.should_block() {
        if let Some((mime, body)) = result
            .redirect
            .as_deref()
            .filter(|redirect| !redirect.is_empty())
            .and_then(parse_data_url)
        {
            return NetworkDecision::Stub { mime, body, info };
        }
        if request.resource_type.is_document_navigation(&request.url) {
            return NetworkDecision::BlockNavigation { info };
        }
        return NetworkDecision::Cancel { info };
    }
    let rewritten_url = if allows_removeparam(&request.method) {
        result
            .rewritten_url
            .as_ref()
            .filter(|url| !url.is_empty())
            .cloned()
    } else {
        None
    };
    NetworkDecision::Allow {
        rewritten_url,
        info,
    }
}

fn allows_removeparam(method: &str) -> bool {
    let method = method.to_ascii_lowercase();
    matches!(method.as_str(), "get" | "head" | "options")
}

fn hosts_equal(url: &str, canonical_host: &str) -> bool {
    let Some(host) = request_host(url) else {
        return false;
    };
    host == canonical_host
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn request_host(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    parsed
        .host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

fn replace_host(url: &str, canonical_host: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;
    parsed.set_host(Some(canonical_host.trim())).ok()?;
    Some(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::BraveAdblockHost;

    #[test]
    fn host_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BraveAdblockHost>();
    }
}
