// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use zen_chromium::cef::on_before_resource_load;
use zen_chromium::filter_resources::Resource;
use zen_chromium::{BraveAdblockHost, ChromiumResourceRequest, ChromiumResourceType};

fn main() {
    let host = BraveAdblockHost::compile(["||ads.example^$script"], Vec::<Resource>::new());
    let request = ChromiumResourceRequest {
        url: "https://ads.example/ad.js".to_string(),
        initiator: Some("https://news.example/".to_string()),
        resource_type: ChromiumResourceType::Script,
        method: "GET".to_string(),
    };
    let decision = on_before_resource_load(&host, &request, None);
    println!("{decision:?}");
}
