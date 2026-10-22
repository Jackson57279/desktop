// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashSet;

use adblock::cosmetic_filter_cache::UrlSpecificResources;
use serde::Serialize;

/// Cosmetic work for one document.
///
/// `procedural_actions` and `injected_script` are inputs for an isolated-world
/// content script. This host does not run them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CosmeticPlan {
    pub hide_selectors: Vec<String>,
    pub procedural_actions: Vec<String>,
    pub exceptions: Vec<String>,
    pub injected_script: String,
    pub generichide: bool,
    pub stylesheet: String,
}

impl CosmeticPlan {
    pub(crate) fn from_resources(
        resources: UrlSpecificResources,
        generic_selectors: Vec<String>,
    ) -> Self {
        let mut hide_selectors: Vec<String> = resources
            .hide_selectors
            .into_iter()
            .chain(generic_selectors)
            .filter(|selector| !selector.is_empty())
            .collect();
        hide_selectors.sort();
        hide_selectors.dedup();

        let procedural_actions = sorted(resources.procedural_actions);
        let exceptions = sorted(resources.exceptions);
        let stylesheet = stylesheet_for(&hide_selectors);

        Self {
            hide_selectors,
            procedural_actions,
            exceptions,
            injected_script: resources.injected_script,
            generichide: resources.generichide,
            stylesheet,
        }
    }
}

fn sorted(values: HashSet<String>) -> Vec<String> {
    let mut values: Vec<String> = values.into_iter().collect();
    values.sort();
    values
}

fn stylesheet_for(selectors: &[String]) -> String {
    let mut css = String::new();
    for selector in selectors {
        css.push_str(selector);
        css.push_str(" { display: none !important; }\n");
    }
    css
}
