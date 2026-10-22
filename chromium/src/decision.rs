// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/// Which filter line matched, when the engine was built with debug info.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchInfo {
    pub rule: Option<String>,
    pub exception: Option<String>,
    pub important: bool,
}

/// What the Chromium network stack should do with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkDecision {
    Allow {
        rewritten_url: Option<String>,
        info: MatchInfo,
    },
    Cancel {
        info: MatchInfo,
    },
    Stub {
        mime: String,
        body: Vec<u8>,
        info: MatchInfo,
    },
    BlockNavigation {
        info: MatchInfo,
    },
}

impl NetworkDecision {
    pub fn allow() -> Self {
        Self::Allow {
            rewritten_url: None,
            info: MatchInfo::default(),
        }
    }

    pub fn blocked(&self) -> bool {
        matches!(
            self,
            Self::Cancel { .. } | Self::Stub { .. } | Self::BlockNavigation { .. }
        )
    }

    pub fn info(&self) -> &MatchInfo {
        match self {
            Self::Allow { info, .. }
            | Self::Cancel { info }
            | Self::Stub { info, .. }
            | Self::BlockNavigation { info } => info,
        }
    }

    pub fn rewritten_url(&self) -> Option<&str> {
        match self {
            Self::Allow { rewritten_url, .. } => rewritten_url.as_deref(),
            Self::Cancel { .. } | Self::Stub { .. } | Self::BlockNavigation { .. } => None,
        }
    }
}
