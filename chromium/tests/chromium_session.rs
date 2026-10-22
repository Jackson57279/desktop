// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use zen_chromium::filter_resources::{MimeType, PermissionMask, Resource, ResourceType};
use zen_chromium::{BraveAdblockHost, ChromiumSession, LaunchOptions};

struct Site {
    base: String,
    hits: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Site {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let accepted_hits = Arc::clone(&hits);
        let accepted_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !accepted_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let hits = Arc::clone(&accepted_hits);
                        thread::spawn(move || serve(stream, hits));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base: format!("http://{addr}"),
            hits,
            stop,
            thread: Some(thread),
        }
    }

    fn clear(&self) {
        self.hits.lock().unwrap().clear();
    }

    fn hits(&self) -> Vec<String> {
        self.hits.lock().unwrap().clone()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.base.trim_start_matches("http://"));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, hits: Arc<Mutex<Vec<String>>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0_u8; 8192];
    let Ok(size) = stream.read(&mut buffer) else {
        return;
    };
    if size == 0 {
        return;
    }
    let request = String::from_utf8_lossy(&buffer[..size]);
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");
    hits.lock().unwrap().push(path.to_string());
    let (status, content_type, body) = match path {
        "/" => (
            "200 OK",
            "text/html; charset=utf-8",
            INDEX.as_bytes().to_vec(),
        ),
        "/app.js" => (
            "200 OK",
            "application/javascript",
            b"window.__app = true;".to_vec(),
        ),
        "/ads/ad.js" => (
            "200 OK",
            "application/javascript",
            b"window.__ad = true;".to_vec(),
        ),
        "/stub-page.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            STUB_PAGE.as_bytes().to_vec(),
        ),
        "/stub.js" => (
            "200 OK",
            "application/javascript",
            b"window.__stub = \"server\";".to_vec(),
        ),
        "/csp-page.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            CSP_PAGE.as_bytes().to_vec(),
        ),
        "/blocked.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            BLOCKED_PAGE.as_bytes().to_vec(),
        ),
        "/favicon.ico" => ("204 No Content", "image/x-icon", Vec::new()),
        _ => ("404 Not Found", "text/plain", b"missing".to_vec()),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(&body);
}

const INDEX: &str = r#"<!doctype html><html><head><title>zen</title></head><body>
<div class="ad-box" id="ad">ad</div>
<div class="banner">banner</div>
<div id="content">content</div>
<script src="/app.js"></script>
<script src="/ads/ad.js"></script>
</body></html>"#;

const STUB_PAGE: &str = r#"<!doctype html><html><head><title>stub</title></head><body><script src="/stub.js"></script></body></html>"#;

const CSP_PAGE: &str = r#"<!doctype html><html><head><title>csp</title></head><body>
<script>window.__inline = true;</script>
<script src="/app.js"></script>
</body></html>"#;

const BLOCKED_PAGE: &str = r#"<!doctype html><html><head><title>secret</title></head><body>
<script>window.__secret = true;</script>
<p>secret page</p>
</body></html>"#;

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

fn host() -> BraveAdblockHost {
    BraveAdblockHost::compile(
        [concat!(
            "/ads/ad.js$script\n",
            "127.0.0.1##.ad-box\n",
            "##.banner\n",
            "127.0.0.1##+js(marker.js)\n",
            "/csp-page.html^$csp=script-src 'none'\n",
            "/stub.js$script,redirect=noopjs\n",
            "/blocked.html$document\n",
            "*$removeparam=utm_source\n",
        )],
        vec![
            javascript("marker.js", "window.__scriptlet = true;"),
            javascript("noopjs", "window.__stub = \"noop\";"),
        ],
    )
}

fn zen_state(origin: &str) -> serde_json::Value {
    let response = ureq::get(&format!("{origin}/api/state")).call().unwrap();
    serde_json::from_str(&response.into_string().unwrap()).unwrap()
}

fn context(session: &ChromiumSession, site: &Site) -> String {
    format!(
        "observed: {:?}\nhits: {:?}",
        session.observed(),
        site.hits()
    )
}

#[test]
fn chromium_enforces_blocks_cosmetics_csp_stubs_and_rewrites() {
    let site = Site::spawn();
    let mut session = ChromiumSession::launch(host(), LaunchOptions::headless())
        .unwrap_or_else(|error| panic!("failed to launch Chromium: {error}"));
    assert!(
        session.engine_version().starts_with("Chromium "),
        "engine is not Chromium: {}",
        session.engine_version()
    );
    let origin = session.zen_origin().to_string();
    let mut sidebar = String::new();
    for _ in 0..25 {
        sidebar = session.sidebar_text().unwrap_or_default();
        if sidebar.contains("New Tab") && sidebar.contains("Space") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        sidebar.contains("New Tab") && sidebar.contains("Space"),
        "sidebar did not render Zen chrome: {sidebar}"
    );
    ureq::post(&format!("{origin}/api/new-space"))
        .set("content-type", "application/json")
        .send_string(r#"{"name":"Work"}"#)
        .unwrap();
    ureq::post(&format!("{origin}/api/compact"))
        .set("content-type", "application/json")
        .send_string("{}")
        .unwrap();
    let chrome_state = zen_state(&origin);
    assert_eq!(chrome_state["spaces"].as_array().unwrap().len(), 2);
    assert!(chrome_state["compact"].as_bool().unwrap());
    assert!(chrome_state["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .any(|space| space["name"] == "Work"));

    session
        .navigate(&format!("{}/?utm_source=bad&id=7", site.base))
        .unwrap_or_else(|error| panic!("{error}\n{}", context(&session, &site)));
    let href = session.evaluate("location.href").unwrap();
    let href = href.as_str().unwrap();
    assert!(
        href.contains("id=7") && !href.contains("utm_source"),
        "href {href}\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session.evaluate("window.__app === true").unwrap(),
        true,
        "app script did not run\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session.evaluate("window.__ad === true").unwrap(),
        false,
        "ad script ran\n{}",
        context(&session, &site)
    );
    assert!(
        session
            .observed()
            .iter()
            .any(|item| item.url.contains("/ads/ad.js") && item.action == "cancel"),
        "ad request was not cancelled\n{}",
        context(&session, &site)
    );
    assert!(
        !site.hits().iter().any(|hit| hit == "/ads/ad.js"),
        "ad request reached the server\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session
            .evaluate("getComputedStyle(document.querySelector('.ad-box')).display")
            .unwrap(),
        "none",
        "specific cosmetic hide missed\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session
            .evaluate("getComputedStyle(document.querySelector('.banner')).display")
            .unwrap(),
        "none",
        "generic cosmetic hide missed\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session.evaluate("window.__scriptlet === true").unwrap(),
        true,
        "scriptlet did not run in the page\n{}",
        context(&session, &site)
    );

    site.clear();
    session
        .navigate(&format!("{}/csp-page.html", site.base))
        .unwrap_or_else(|error| panic!("{error}\n{}", context(&session, &site)));
    assert_eq!(
        session.evaluate("window.__inline === true").unwrap(),
        false,
        "CSP did not block the inline script\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session.evaluate("window.__app === true").unwrap(),
        false,
        "CSP did not block the external script\n{}",
        context(&session, &site)
    );

    site.clear();
    session
        .navigate(&format!("{}/stub-page.html", site.base))
        .unwrap_or_else(|error| panic!("{error}\n{}", context(&session, &site)));
    assert_eq!(
        session.evaluate("window.__stub").unwrap(),
        "noop",
        "redirect stub was not the script body\n{}",
        context(&session, &site)
    );
    assert!(
        !site.hits().iter().any(|hit| hit == "/stub.js"),
        "stub script was fetched from the server\n{}",
        context(&session, &site)
    );

    site.clear();
    session
        .navigate(&format!("{}/blocked.html", site.base))
        .unwrap_or_else(|error| panic!("{error}\n{}", context(&session, &site)));
    let text = session.evaluate("document.body.innerText").unwrap();
    let text = text.as_str().unwrap_or("");
    assert!(
        text.contains("Blocked by Zen"),
        "interstitial missing: {text}\n{}",
        context(&session, &site)
    );
    assert_eq!(
        session.evaluate("window.__secret === true").unwrap(),
        false,
        "blocked document still ran\n{}",
        context(&session, &site)
    );
    assert!(
        !site.hits().iter().any(|hit| hit == "/blocked.html"),
        "blocked navigation reached the server\n{}",
        context(&session, &site)
    );
}
