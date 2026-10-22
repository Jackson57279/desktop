// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Chromium embedder hooks.
//!
//! These functions are what `CefResourceRequestHandler::OnBeforeResourceLoad`,
//! response-header handling, and `CefLoadHandler::OnLoadEnd` call. This module
//! does not link CEF.

use crate::cosmetic::CosmeticPlan;
use crate::decision::NetworkDecision;
use crate::host::BraveAdblockHost;
use crate::request::ChromiumResourceRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CefBeforeLoad {
    /// `RV_CONTINUE`. `url_override` is a `$removeparam` rewrite.
    Continue { url_override: Option<String> },
    /// `RV_CANCEL` / `net::ERR_BLOCKED_BY_CLIENT`.
    Cancel,
    /// `GetResourceHandler` returns this body and the request stays off the network.
    Replace { mime: String, body: Vec<u8> },
    /// Main-frame document block. Show the navigation interstitial instead of
    /// failing a subresource.
    BlockNavigation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CefCosmeticInjection {
    pub stylesheet: String,
    pub script: String,
    pub procedural_actions: Vec<String>,
    pub generichide: bool,
}

#[must_use]
pub fn on_before_resource_load(
    host: &BraveAdblockHost,
    request: &ChromiumResourceRequest,
    canonical_host: Option<&str>,
) -> CefBeforeLoad {
    match host.decide_with_canonical_host(request, canonical_host) {
        NetworkDecision::Allow { rewritten_url, .. } => CefBeforeLoad::Continue {
            url_override: rewritten_url,
        },
        NetworkDecision::Cancel { .. } => CefBeforeLoad::Cancel,
        NetworkDecision::Stub { mime, body, .. } => CefBeforeLoad::Replace { mime, body },
        NetworkDecision::BlockNavigation { .. } => CefBeforeLoad::BlockNavigation,
    }
}

#[must_use]
pub fn on_headers_received(
    host: &BraveAdblockHost,
    request: &ChromiumResourceRequest,
) -> Option<String> {
    host.content_security_policy(request)
}

#[must_use]
pub fn on_load_end(host: &BraveAdblockHost, url: &str) -> CefCosmeticInjection {
    injection(host.cosmetics(url))
}

#[must_use]
pub fn on_dom_tokens<C, I>(
    host: &BraveAdblockHost,
    url: &str,
    classes: &[C],
    ids: &[I],
) -> CefCosmeticInjection
where
    C: AsRef<str>,
    I: AsRef<str>,
{
    injection(host.cosmetics_for_dom(url, classes, ids))
}

fn injection(plan: CosmeticPlan) -> CefCosmeticInjection {
    CefCosmeticInjection {
        stylesheet: plan.stylesheet,
        script: plan.injected_script,
        procedural_actions: plan.procedural_actions,
        generichide: plan.generichide,
    }
}
