// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Chromium request host for Zen, using Brave Browser's adblock engine.
//!
//! The Firefox chrome under `src/` is unchanged. A Chromium embedder calls
//! [`cef::on_before_resource_load`] when a request starts,
//! [`cef::on_headers_received`] when response headers arrive, and
//! [`cef::on_load_end`] plus [`cef::on_dom_tokens`] for cosmetic filtering.
//!
//! The design lives in `docs/superpowers/specs/2026-10-22-chromium-brave-adblock-design.md`.

pub mod cef;
pub mod cosmetic;
pub mod data_url;
pub mod decision;
pub mod filter_resources;
pub mod host;
pub mod lists;
pub mod request;
pub mod resource_type;

pub use cosmetic::CosmeticPlan;
pub use decision::{MatchInfo, NetworkDecision};
pub use host::{BraveAdblockHost, HostError};
pub use lists::{read_filter_list, read_filter_lists, ListError, MAX_FILTER_LIST_BYTES};
pub use request::ChromiumResourceRequest;
pub use resource_type::{ChromiumResourceType, UnknownResourceType};
