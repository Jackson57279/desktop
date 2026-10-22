// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use zen_chromium::cef::{
    on_before_resource_load, on_dom_tokens, on_headers_received, on_load_end, CefBeforeLoad,
};
use zen_chromium::filter_resources::{MimeType, PermissionMask, Resource, ResourceType};
use zen_chromium::{
    BraveAdblockHost, ChromiumResourceRequest, ChromiumResourceType, NetworkDecision,
};

fn host(rules: &str) -> BraveAdblockHost {
    BraveAdblockHost::compile([rules], Vec::new())
}

fn host_with(rules: &str, resources: Vec<Resource>) -> BraveAdblockHost {
    BraveAdblockHost::compile([rules], resources)
}

fn request(
    url: &str,
    source: Option<&str>,
    resource_type: ChromiumResourceType,
    method: &str,
) -> ChromiumResourceRequest {
    ChromiumResourceRequest {
        url: url.to_string(),
        initiator: source.map(str::to_string),
        resource_type,
        method: method.to_string(),
    }
}

fn script(url: &str, source: &str) -> ChromiumResourceRequest {
    request(url, Some(source), ChromiumResourceType::Script, "GET")
}

fn javascript(name: &str, body: &str) -> Resource {
    Resource {
        name: name.to_string(),
        aliases: Vec::new(),
        kind: ResourceType::Mime(MimeType::ApplicationJavascript),
        content: STANDARD.encode(body),
        dependencies: Vec::new(),
        permission: PermissionMask::default(),
    }
}

#[test]
fn blocks_third_party_script() {
    let decision = host("||ads.example^$script").decide(&script(
        "https://ads.example/ad.js",
        "https://news.example/",
    ));
    assert!(matches!(decision, NetworkDecision::Cancel { .. }));
    assert_eq!(
        decision.info().rule.as_deref(),
        Some("||ads.example^$script")
    );
}

#[test]
fn first_party_does_not_match_third_party_option() {
    let decision = host("||ads.example^$third-party").decide(&script(
        "https://ads.example/ad.js",
        "https://www.ads.example/",
    ));
    assert!(!decision.blocked());
}

#[test]
fn exception_cancels_a_block() {
    let decision = host("||ads.example^\n@@||ads.example/allowed.js^").decide(&script(
        "https://ads.example/allowed.js",
        "https://news.example/",
    ));
    assert!(!decision.blocked());
    assert!(decision.info().exception.is_some());
}

#[test]
fn important_beats_exception() {
    let decision = host("||ads.example^$important\n@@||ads.example^").decide(&script(
        "https://ads.example/ad.js",
        "https://news.example/",
    ));
    assert!(matches!(decision, NetworkDecision::Cancel { .. }));
    assert!(decision.info().important);
    assert!(decision.info().exception.is_none());
}

#[test]
fn redirect_blocks_with_a_stub_body() {
    let decision = host_with(
        "||ads.example/ad.js$script,redirect=noopjs",
        vec![javascript("noopjs", "noop")],
    )
    .decide(&script(
        "https://ads.example/ad.js",
        "https://news.example/",
    ));
    match decision {
        NetworkDecision::Stub { mime, body, .. } => {
            assert_eq!(mime, "application/javascript");
            assert_eq!(body, b"noop");
        }
        other => panic!("expected stub, got {other:?}"),
    }
}

#[test]
fn redirect_rule_does_not_block_by_itself() {
    let alone = host_with(
        "||ads.example/ad.js$script,redirect-rule=noopjs",
        vec![javascript("noopjs", "noop")],
    )
    .decide(&script(
        "https://ads.example/ad.js",
        "https://news.example/",
    ));
    assert!(!alone.blocked());

    let with_block = host_with(
        "||ads.example^$script\n||ads.example/ad.js$script,redirect-rule=noopjs",
        vec![javascript("noopjs", "noop")],
    )
    .decide(&script(
        "https://ads.example/ad.js",
        "https://news.example/",
    ));
    assert!(matches!(with_block, NetworkDecision::Stub { .. }));
}

#[test]
fn csp_is_not_a_network_block() {
    let rules = "||news.example^$csp=script-src 'none'";
    let document = request(
        "https://news.example/",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    let engine = host(rules);
    assert!(!engine.decide(&document).blocked());
    assert_eq!(
        engine.content_security_policy(&document).as_deref(),
        Some("script-src 'none'")
    );

    let script_request = script("https://news.example/app.js", "https://news.example/");
    assert!(engine.content_security_policy(&script_request).is_none());
}

#[test]
fn csp_exception_disables_injection() {
    let engine = host("||news.example^$csp=script-src 'none'\n@@||news.example^$csp");
    let document = request(
        "https://news.example/",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    assert!(engine.content_security_policy(&document).is_none());
}

#[test]
fn removeparam_rewrites_get_and_ignores_post() {
    let engine = host("*$removeparam=utm_source");
    let get = request(
        "https://news.example/a?utm_source=x&id=1",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    let rewritten = engine.decide(&get).rewritten_url().unwrap().to_string();
    assert!(rewritten.contains("id=1"), "{rewritten}");
    assert!(!rewritten.contains("utm_source"), "{rewritten}");
    assert!(!engine.decide(&get).blocked());

    let post = request(
        "https://news.example/a?utm_source=x&id=1",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "POST",
    );
    assert!(engine.decide(&post).rewritten_url().is_none());
}

#[test]
fn cosmetic_hide_does_not_block_network() {
    let engine = host("example.com##.ad-box");
    let plan = engine.cosmetics("https://example.com/");
    assert!(plan
        .hide_selectors
        .iter()
        .any(|selector| selector == ".ad-box"));
    assert!(plan
        .stylesheet
        .contains(".ad-box { display: none !important; }"));
    assert!(!engine
        .decide(&script(
            "https://example.com/app.js",
            "https://example.com/"
        ))
        .blocked());
    assert!(engine
        .cosmetics("https://other.example/")
        .hide_selectors
        .is_empty());
}

#[test]
fn generic_class_hide_needs_dom_tokens_and_respects_exceptions() {
    let generic = host("##.banner");
    assert!(generic
        .cosmetics("https://example.com/")
        .stylesheet
        .is_empty());
    let with_token = generic.cosmetics_for_dom("https://example.com/", &["banner"], &[] as &[&str]);
    assert!(with_token.stylesheet.contains(".banner"));

    let excepted = host("##.banner\nexample.com#@#.banner");
    let plan = excepted.cosmetics_for_dom("https://example.com/", &["banner"], &[] as &[&str]);
    assert!(!plan.stylesheet.contains("banner"));
    assert!(plan
        .exceptions
        .iter()
        .any(|selector| selector.contains("banner")));
}

#[test]
fn generichide_skips_generic_class_selectors() {
    let engine = host("##.banner\n@@||example.com^$generichide");
    let plan = engine.cosmetics_for_dom("https://example.com/", &["banner"], &[] as &[&str]);
    assert!(plan.generichide);
    assert!(!plan.stylesheet.contains("banner"));
}

#[test]
fn scriptlet_is_returned_and_not_executed() {
    let engine = host_with(
        "example.com##+js(nowebrtc.js)",
        vec![javascript("nowebrtc.js", "nowebrtc.js")],
    );
    let plan = engine.cosmetics("https://example.com/");
    assert!(plan.injected_script.contains("try {"));
    assert!(plan.injected_script.contains("nowebrtc.js"));
}

#[test]
fn privileged_scriptlet_is_not_injected_for_default_lists() {
    let mut resource = javascript("nowebrtc.js", "nowebrtc.js");
    resource.permission = PermissionMask::from_bits(1);
    let engine = host_with("example.com##+js(nowebrtc.js)", vec![resource]);
    assert!(engine
        .cosmetics("https://example.com/")
        .injected_script
        .is_empty());
}

#[test]
fn image_rule_blocks_favicon_and_not_script() {
    let engine = host("||cdn.example^$image");
    let favicon = request(
        "https://cdn.example/icon.ico",
        Some("https://news.example/"),
        ChromiumResourceType::Favicon,
        "GET",
    );
    assert!(engine.decide(&favicon).blocked());
    assert!(!engine
        .decide(&script(
            "https://cdn.example/app.js",
            "https://news.example/"
        ))
        .blocked());
}

#[test]
fn worker_is_not_a_script() {
    let engine = host("||cdn.example^$script");
    let worker = request(
        "https://cdn.example/worker.js",
        Some("https://news.example/"),
        ChromiumResourceType::Worker,
        "GET",
    );
    assert!(!engine.decide(&worker).blocked());
}

#[test]
fn main_frame_block_is_a_navigation() {
    let engine = host("||ads.example^");
    let document = request(
        "https://ads.example/",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    assert!(matches!(
        engine.decide(&document),
        NetworkDecision::BlockNavigation { .. }
    ));
    assert!(matches!(
        engine.decide(&script(
            "https://ads.example/ad.js",
            "https://news.example/"
        )),
        NetworkDecision::Cancel { .. }
    ));
}

#[test]
fn websocket_on_a_main_frame_cancels_instead_of_navigating() {
    let engine = host("||ads.example^$websocket");
    let socket = request(
        "wss://ads.example/socket",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    assert!(matches!(
        engine.decide(&socket),
        NetworkDecision::Cancel { .. }
    ));
}

#[test]
fn unsupported_schemes_are_allowed() {
    let engine = host("*$script");
    let file = request(
        "file:///tmp/ad.js",
        Some("https://news.example/"),
        ChromiumResourceType::Script,
        "GET",
    );
    assert!(!engine.decide(&file).blocked());
}

#[test]
fn canonical_host_can_block_and_does_not_apply_its_removeparam() {
    let engine = host("||tracker.example^$script\n*$removeparam=utm_source");
    let request = request(
        "https://cdn.example/ad.js?utm_source=x",
        Some("https://news.example/"),
        ChromiumResourceType::Script,
        "GET",
    );
    assert!(!engine.decide(&request).blocked());
    let uncloaked = engine.decide_with_canonical_host(&request, Some("tracker.example"));
    assert!(matches!(uncloaked, NetworkDecision::Cancel { .. }));
    assert!(uncloaked.rewritten_url().is_none());
}

#[test]
fn canonical_host_keeps_the_original_removeparam_when_it_does_not_block() {
    let engine = host("*$removeparam=utm_source");
    let request = request(
        "https://news.example/a?utm_source=x&id=1",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    let decision = engine.decide_with_canonical_host(&request, Some("news.example"));
    let rewritten = decision.rewritten_url().unwrap();
    assert!(rewritten.contains("id=1"));
    assert!(!rewritten.contains("utm_source"));
}

#[test]
fn regex_rule_blocks() {
    let decision = host(r"/tracking\.js/$script").decide(&script(
        "https://cdn.example/tracking.js",
        "https://news.example/",
    ));
    assert!(decision.blocked());
}

#[test]
fn dat_cache_round_trip_keeps_rules_but_not_resources() {
    let resource = javascript("noopjs", "noop");
    let host = host_with(
        "||ads.example/ad.js$script,redirect=noopjs",
        vec![resource.clone()],
    );
    let bytes = host.serialize();
    let restored = BraveAdblockHost::from_dat(&bytes, vec![resource]).unwrap();
    assert!(matches!(
        restored.decide(&script(
            "https://ads.example/ad.js",
            "https://news.example/"
        )),
        NetworkDecision::Stub { .. }
    ));
    let without_resources = BraveAdblockHost::from_dat(&bytes, Vec::new()).unwrap();
    assert!(matches!(
        without_resources.decide(&script(
            "https://ads.example/ad.js",
            "https://news.example/"
        )),
        NetworkDecision::Cancel { .. }
    ));
    assert!(BraveAdblockHost::from_dat(b"not-a-dat", Vec::new()).is_err());
}

#[test]
fn cef_hooks_map_block_rewrite_and_cosmetics() {
    let engine = host("||ads.example^\n*$removeparam=utm_source\nexample.com##.ad-box");
    let blocked = on_before_resource_load(
        &engine,
        &script("https://ads.example/ad.js", "https://news.example/"),
        None,
    );
    assert_eq!(blocked, CefBeforeLoad::Cancel);

    let document = request(
        "https://news.example/a?utm_source=x&id=1",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    match on_before_resource_load(&engine, &document, None) {
        CefBeforeLoad::Continue {
            url_override: Some(url),
        } => {
            assert!(url.contains("id=1"));
            assert!(!url.contains("utm_source"));
        }
        other => panic!("expected rewrite, got {other:?}"),
    }

    let navigation = request(
        "https://ads.example/",
        Some("https://news.example/"),
        ChromiumResourceType::MainFrame,
        "GET",
    );
    assert_eq!(
        on_before_resource_load(&engine, &navigation, None),
        CefBeforeLoad::BlockNavigation
    );
    assert!(on_headers_received(&engine, &document).is_none());

    let injection = on_load_end(&engine, "https://example.com/");
    assert!(injection.stylesheet.contains(".ad-box"));
    assert!(injection.script.is_empty());

    let generic = host("##.banner");
    let tokens = on_dom_tokens(
        &generic,
        "https://example.com/",
        &["banner"],
        &[] as &[&str],
    );
    assert!(tokens.stylesheet.contains(".banner"));
}

#[test]
fn missing_initiator_is_third_party() {
    let decision = host("||ads.example^$third-party").decide(&request(
        "https://ads.example/ad.js",
        None,
        ChromiumResourceType::Script,
        "GET",
    ));
    assert!(decision.blocked());
}

#[test]
fn reads_lists_from_disk() {
    let path = std::env::temp_dir().join(format!("zen-chromium-list-{}.txt", std::process::id()));
    std::fs::write(&path, "||ads.example^$script\n").unwrap();
    let engine = BraveAdblockHost::from_files(std::slice::from_ref(&path), Vec::new()).unwrap();
    assert!(engine
        .decide(&script(
            "https://ads.example/ad.js",
            "https://news.example/"
        ))
        .blocked());
    let _ = std::fs::remove_file(path);
}
