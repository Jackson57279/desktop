// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::resource_type::ChromiumResourceType;

/// One Chromium request, in the shape Brave's browser-process loader sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromiumResourceRequest {
    pub url: String,
    /// Initiator URL. `None` is third-party, matching a missing Chromium initiator.
    pub initiator: Option<String>,
    pub resource_type: ChromiumResourceType,
    /// HTTP method. Empty means unknown, and `$removeparam` will not apply.
    pub method: String,
}
