// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A real Chromium process with Brave's adblock engine in the request path.
//!
//! Chromium pauses every request on the DevTools Fetch domain. This session
//! makes the same decision Brave's browser-process loader makes, then tells
//! Chromium to continue, rewrite, cancel, or fulfill the response.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};

use crate::cdp::{Cdp, Incoming};
use crate::cef::{
    on_before_resource_load, on_dom_tokens, on_headers_received, on_load_end, CefBeforeLoad,
};
use crate::engine::{configure_process, ensure_chromium_engine};
use crate::host::BraveAdblockHost;
use crate::request::ChromiumResourceRequest;
use crate::resource_type::{is_supported_scheme, ChromiumResourceType};

const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(20);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub enum BrowserError {
    Launch(String),
    Cdp(String),
    Timeout(String),
    Io(std::io::Error),
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(message) | Self::Cdp(message) | Self::Timeout(message) => {
                formatter.write_str(message)
            }
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Launch(_) | Self::Cdp(_) | Self::Timeout(_) => None,
        }
    }
}

impl From<std::io::Error> for BrowserError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservedRequest {
    pub url: String,
    pub resource_type: String,
    pub action: String,
}

pub struct LaunchOptions {
    pub binary: Option<PathBuf>,
    pub headless: bool,
}

impl LaunchOptions {
    pub fn headless() -> Self {
        Self {
            binary: None,
            headless: true,
        }
    }

    pub fn headed() -> Self {
        Self {
            binary: None,
            headless: false,
        }
    }

    pub fn binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = Some(binary.into());
        self
    }
}

pub struct ChromiumSession {
    child: Child,
    user_data: PathBuf,
    cdp: Cdp,
    host: BraveAdblockHost,
    main_frame_id: String,
    observed: Vec<ObservedRequest>,
    version: String,
}

impl ChromiumSession {
    pub fn launch(host: BraveAdblockHost, options: LaunchOptions) -> Result<Self, BrowserError> {
        let engine = ensure_chromium_engine(options.binary.as_deref())
            .map_err(|error| BrowserError::Launch(error.to_string()))?;
        let binary = engine.binary;
        let user_data = std::env::temp_dir().join(format!(
            "zen-chromium-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&user_data)?;
        let stderr_path = user_data.join("stderr.log");
        let stderr = fs::File::create(&stderr_path)?;
        let mut command = Command::new(&binary);
        configure_process(&mut command, &binary);
        command
            .arg("--remote-debugging-port=0")
            .arg("--remote-allow-origins=*")
            .arg(format!("--user-data-dir={}", user_data.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-extensions")
            .arg("--disable-sync")
            .arg("--disable-background-networking")
            .arg("--disable-component-update")
            .arg("--disable-default-apps")
            .arg("--disable-translate")
            .arg("--disable-popup-blocking")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-gpu")
            .arg("--window-size=1280,800");
        if options.headless {
            command.arg("--headless=new");
        }
        command
            .arg("about:blank")
            .env("HOME", &user_data)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command.spawn().map_err(|error| {
            BrowserError::Launch(format!("failed to start {}: {error}", binary.display()))
        })?;
        let port = match wait_for_port(&user_data, &stderr_path) {
            Ok(port) => port,
            Err(error) => {
                shutdown(child);
                return Err(error);
            }
        };
        trace(format!("devtools port {port}"));
        let session = (|| {
            let page_socket = page_websocket(port, &stderr_path)?;
            let mut cdp = Cdp::connect(&page_socket)?;
            cdp.call("Page.enable", json!({}))?;
            cdp.call("Network.enable", json!({}))?;
            cdp.call("Network.setCacheDisabled", json!({"cacheDisabled": true}))?;
            cdp.call(
                "Fetch.enable",
                json!({
                    "patterns": [
                        {"requestStage": "Request"},
                        {"requestStage": "Response", "resourceType": "Document"}
                    ]
                }),
            )?;
            let tree = cdp.call("Page.getFrameTree", json!({}))?;
            let main_frame_id = tree["frameTree"]["frame"]["id"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if main_frame_id.is_empty() {
                return Err(BrowserError::Cdp(
                    "Chromium did not report a main frame".to_string(),
                ));
            }
            Ok((cdp, main_frame_id))
        })();
        let (cdp, main_frame_id) = match session {
            Ok(ready) => ready,
            Err(error) => {
                shutdown(child);
                return Err(error);
            }
        };
        Ok(Self {
            child,
            user_data,
            cdp,
            host,
            main_frame_id,
            observed: Vec::new(),
            version: engine.version,
        })
    }

    pub fn engine_version(&self) -> &str {
        &self.version
    }

    pub fn navigate(&mut self, url: &str) -> Result<(), BrowserError> {
        self.observed.clear();
        self.drain_stale_events();
        let navigation_id = self.cdp.send("Page.navigate", json!({"url": url}))?;
        let started = Instant::now();
        let mut navigated = false;
        let mut loaded = false;
        while !loaded {
            if started.elapsed() > NAVIGATION_TIMEOUT {
                return Err(BrowserError::Timeout(format!(
                    "navigation to {url} timed out; observed {:?}",
                    self.observed
                )));
            }
            match self.cdp.poll()? {
                None => continue,
                Some(Incoming::Result { id, result }) if id == navigation_id => {
                    if let Some(error) = result.get("errorText").and_then(Value::as_str) {
                        trace(format!("navigate errorText: {error}"));
                    }
                }
                Some(Incoming::Failure { id, message }) if id == navigation_id => {
                    return Err(BrowserError::Cdp(format!(
                        "navigation to {url} failed: {message}"
                    )));
                }
                Some(Incoming::Event { method, params }) => {
                    if method == "Page.frameNavigated"
                        && params["frame"]["id"].as_str() == Some(self.main_frame_id.as_str())
                        && params["frame"]["url"].as_str() != Some("about:blank")
                    {
                        navigated = true;
                    }
                    if navigated {
                        loaded = self.on_event(&method, &params)?;
                    } else if method == "Fetch.requestPaused" {
                        self.on_paused(params)?;
                    }
                }
                Some(Incoming::Result { .. } | Incoming::Failure { .. }) => {}
            }
        }
        let final_url = self
            .evaluate("location.href")?
            .as_str()
            .unwrap_or(url)
            .to_string();
        self.apply_dom_cosmetics(&final_url)?;
        Ok(())
    }

    pub fn evaluate(&mut self, expression: &str) -> Result<Value, BrowserError> {
        let result = self.cdp.call(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;
        if result.get("exceptionDetails").is_some() {
            return Err(BrowserError::Cdp(format!(
                "page evaluation failed: {}",
                result["exceptionDetails"]
            )));
        }
        Ok(result
            .get("result")
            .and_then(|remote| remote.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub fn observed(&self) -> &[ObservedRequest] {
        &self.observed
    }

    pub fn run_until_exit(&mut self) -> Result<(), BrowserError> {
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            match self.cdp.poll()? {
                Some(Incoming::Event { method, params }) => {
                    let _ = self.on_event(&method, &params)?;
                }
                Some(Incoming::Result { .. } | Incoming::Failure { .. }) | None => {}
            }
        }
    }

    fn drain_stale_events(&mut self) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(200) {
            match self.cdp.poll() {
                Ok(None) => break,
                Ok(Some(_)) => continue,
                Err(_) => break,
            }
        }
    }

    fn on_event(&mut self, method: &str, params: &Value) -> Result<bool, BrowserError> {
        match method {
            "Fetch.requestPaused" => {
                self.on_paused(params.clone())?;
                Ok(false)
            }
            "Page.loadEventFired" => Ok(true),
            "Page.frameStoppedLoading" => {
                Ok(params["frameId"].as_str() == Some(self.main_frame_id.as_str()))
            }
            _ => Ok(false),
        }
    }

    fn on_paused(&mut self, params: Value) -> Result<(), BrowserError> {
        let request_id = json_string(&params, "requestId");
        let url = json_string(&params["request"], "url");
        let method = json_string(&params["request"], "method");
        let devtools_type = json_string(&params, "resourceType");
        let frame_id = json_string(&params, "frameId");
        let is_main_frame = frame_id == self.main_frame_id;
        let response_stage = params.get("responseStatusCode").is_some();
        if response_stage {
            return self.on_document_response(params, &request_id, &url, &method, is_main_frame);
        }
        self.on_request(
            &request_id,
            &url,
            &method,
            &devtools_type,
            is_main_frame,
            &params,
        )
    }

    fn on_request(
        &mut self,
        request_id: &str,
        url: &str,
        method: &str,
        devtools_type: &str,
        is_main_frame: bool,
        params: &Value,
    ) -> Result<(), BrowserError> {
        if !is_supported_scheme(url) {
            self.record(url, devtools_type, "allow");
            self.cdp
                .call("Fetch.continueRequest", json!({"requestId": request_id}))?;
            return Ok(());
        }
        let initiator = header_value(&params["request"]["headers"], "referer");
        let resource_type = ChromiumResourceType::from_devtools(devtools_type, is_main_frame);
        let request = ChromiumResourceRequest {
            url: url.to_string(),
            initiator,
            resource_type,
            method: if method.is_empty() {
                "GET".to_string()
            } else {
                method.to_string()
            },
        };
        let decision = on_before_resource_load(&self.host, &request, None);
        trace(format!("{devtools_type} {url} -> {decision:?}"));
        match decision {
            CefBeforeLoad::Continue { url_override } => {
                if let Some(rewritten) = url_override.filter(|rewritten| rewritten != url) {
                    self.record(url, devtools_type, "rewrite");
                    self.cdp.call(
                        "Fetch.fulfillRequest",
                        json!({
                            "requestId": request_id,
                            "responseCode": 307,
                            "responseHeaders": [
                                {"name": "Location", "value": rewritten},
                                {"name": "Cache-Control", "value": "no-store"}
                            ],
                        }),
                    )?;
                } else {
                    self.record(url, devtools_type, "allow");
                    self.cdp
                        .call("Fetch.continueRequest", json!({"requestId": request_id}))?;
                }
            }
            CefBeforeLoad::Cancel => {
                self.record(url, devtools_type, "cancel");
                self.cdp.call(
                    "Fetch.failRequest",
                    json!({"requestId": request_id, "errorReason": "BlockedByClient"}),
                )?;
            }
            CefBeforeLoad::Replace { mime, body } => {
                self.record(url, devtools_type, "stub");
                self.cdp.call(
                    "Fetch.fulfillRequest",
                    json!({
                        "requestId": request_id,
                        "responseCode": 200,
                        "responseHeaders": [
                            {"name": "Content-Type", "value": mime},
                            {"name": "Content-Length", "value": body.len().to_string()},
                            {"name": "Cache-Control", "value": "no-store"}
                        ],
                        "body": STANDARD.encode(body),
                    }),
                )?;
            }
            CefBeforeLoad::BlockNavigation => {
                self.record(url, devtools_type, "block_navigation");
                let page = interstitial(url);
                self.cdp.call(
                    "Fetch.fulfillRequest",
                    json!({
                        "requestId": request_id,
                        "responseCode": 200,
                        "responseHeaders": [
                            {"name": "Content-Type", "value": "text/html; charset=utf-8"},
                            {"name": "Content-Length", "value": page.len().to_string()},
                            {"name": "Cache-Control", "value": "no-store"}
                        ],
                        "body": STANDARD.encode(page.as_bytes()),
                    }),
                )?;
            }
        }
        Ok(())
    }

    fn on_document_response(
        &mut self,
        params: Value,
        request_id: &str,
        url: &str,
        method: &str,
        is_main_frame: bool,
    ) -> Result<(), BrowserError> {
        let resource_type = if is_main_frame {
            ChromiumResourceType::MainFrame
        } else {
            ChromiumResourceType::SubFrame
        };
        let request = ChromiumResourceRequest {
            url: url.to_string(),
            initiator: header_value(&params["request"]["headers"], "referer"),
            resource_type,
            method: if method.is_empty() {
                "GET".to_string()
            } else {
                method.to_string()
            },
        };
        let csp = on_headers_received(&self.host, &request);
        let cosmetics = on_load_end(&self.host, url);
        let rewrite_html = !cosmetics.stylesheet.is_empty() || !cosmetics.script.is_empty();
        let status = params["responseStatusCode"].as_u64().unwrap_or(200);
        if csp.is_none() && !rewrite_html {
            self.record(url, "document", "response");
            self.cdp
                .call("Fetch.continueResponse", json!({"requestId": request_id}))?;
            return Ok(());
        }
        let mut headers = header_pairs(&params["responseHeaders"]);
        if let Some(directives) = csp {
            headers.push(("Content-Security-Policy".to_string(), directives));
        }
        if !rewrite_html {
            self.record(url, "document", "csp");
            self.cdp.call(
                "Fetch.continueResponse",
                json!({
                    "requestId": request_id,
                    "responseCode": status,
                    "responseHeaders": header_entries(&headers),
                }),
            )?;
            return Ok(());
        }
        let body = match self
            .cdp
            .call("Fetch.getResponseBody", json!({"requestId": request_id}))
        {
            Ok(body) => decode_cdp_body(&body),
            Err(error) => {
                trace(format!("response body unavailable for {url}: {error}"));
                self.record(url, "document", "csp");
                self.cdp.call(
                    "Fetch.continueResponse",
                    json!({
                        "requestId": request_id,
                        "responseCode": status,
                        "responseHeaders": header_entries(&headers),
                    }),
                )?;
                return Ok(());
            }
        };
        let content_type = header_lookup(&headers, "content-type").unwrap_or("");
        let body = if is_html(content_type, &body) {
            inject_document(
                &String::from_utf8_lossy(&body),
                &cosmetics.stylesheet,
                &cosmetics.script,
            )
            .into_bytes()
        } else {
            body
        };
        headers.retain(|(name, _)| !name.eq_ignore_ascii_case("content-encoding"));
        upsert_header(&mut headers, "content-length", &body.len().to_string());
        if header_lookup(&headers, "content-type").is_none() {
            headers.push((
                "Content-Type".to_string(),
                "text/html; charset=utf-8".to_string(),
            ));
        }
        self.record(url, "document", "rewrite_document");
        self.cdp.call(
            "Fetch.fulfillRequest",
            json!({
                "requestId": request_id,
                "responseCode": status,
                "responseHeaders": header_entries(&headers),
                "body": STANDARD.encode(&body),
            }),
        )?;
        Ok(())
    }

    fn apply_dom_cosmetics(&mut self, url: &str) -> Result<(), BrowserError> {
        let tokens = self.evaluate(
            "(() => { const classes = new Set(); const ids = new Set(); for (const el of document.querySelectorAll('*')) { for (const name of el.classList) classes.add(name); if (el.id) ids.add(el.id); } return {classes: [...classes], ids: [...ids]}; })()",
        )?;
        let classes = string_list(tokens.get("classes"));
        let ids = string_list(tokens.get("ids"));
        let plan = on_dom_tokens(&self.host, url, &classes, &ids);
        if plan.stylesheet.is_empty() {
            return Ok(());
        }
        let css = serde_json::to_string(&plan.stylesheet)
            .map_err(|error| BrowserError::Cdp(error.to_string()))?;
        self.evaluate(&format!(
            "(() => {{ const css = {css}; let style = document.querySelector('style[data-zen-chromium=\"dom\"]'); if (!style) {{ style = document.createElement('style'); style.setAttribute('data-zen-chromium', 'dom'); (document.head || document.documentElement).appendChild(style); }} style.textContent = css; return true; }})()"
        ))?;
        Ok(())
    }

    fn record(&mut self, url: &str, resource_type: &str, action: &str) {
        self.observed.push(ObservedRequest {
            url: url.to_string(),
            resource_type: resource_type.to_string(),
            action: action.to_string(),
        });
    }
}

impl Drop for ChromiumSession {
    fn drop(&mut self) {
        kill_child(self.child.id());
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.user_data);
    }
}

pub fn find_chromium_binary() -> Result<PathBuf, BrowserError> {
    ensure_chromium_engine(None)
        .map(|engine| engine.binary)
        .map_err(|error| BrowserError::Launch(error.to_string()))
}

fn wait_for_port(user_data: &Path, stderr_path: &Path) -> Result<u16, BrowserError> {
    let started = Instant::now();
    let port_file = user_data.join("DevToolsActivePort");
    loop {
        if let Ok(text) = fs::read_to_string(&port_file) {
            if let Some(port) = text.lines().next().and_then(|line| line.parse().ok()) {
                return Ok(port);
            }
        }
        if started.elapsed() > LAUNCH_TIMEOUT {
            return Err(BrowserError::Launch(format!(
                "Chromium did not open a DevTools port. {}",
                stderr_tail(stderr_path)
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn page_websocket(port: u16, stderr_path: &Path) -> Result<String, BrowserError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();
    let started = Instant::now();
    loop {
        let url = format!("http://127.0.0.1:{port}/json/list");
        if let Ok(response) = agent.get(&url).call() {
            if let Ok(text) = response.into_string() {
                if let Ok(targets) = serde_json::from_str::<Vec<Value>>(&text) {
                    if let Some(socket) = targets.iter().find_map(|target| {
                        if target["type"].as_str() == Some("page") {
                            target["webSocketDebuggerUrl"].as_str().map(str::to_string)
                        } else {
                            None
                        }
                    }) {
                        return Ok(socket);
                    }
                }
            }
        }
        if started.elapsed() > LAUNCH_TIMEOUT {
            return Err(BrowserError::Launch(format!(
                "Chromium did not publish a page target. {}",
                stderr_tail(stderr_path)
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn stderr_tail(path: &Path) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let start = text.len().saturating_sub(1200);
    format!("stderr: {}", &text[start..])
}

fn shutdown(mut child: Child) {
    kill_child(child.id());
    let _ = child.wait();
}

fn kill_child(pid: u32) {
    if pid <= 1 {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

fn interstitial(url: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Blocked</title></head><body><h1>Blocked by Zen</h1><p>This navigation was blocked.</p><p>{url}</p></body></html>"
    )
}

pub(crate) fn inject_document(html: &str, stylesheet: &str, script: &str) -> String {
    let mut snippet = String::new();
    if !stylesheet.is_empty() {
        snippet.push_str("<style data-zen-chromium=\"start\">");
        snippet.push_str(&stylesheet.replace("</", "<\\/"));
        snippet.push_str("</style>");
    }
    if !script.is_empty() {
        snippet.push_str("<script data-zen-chromium=\"start\">");
        snippet.push_str(&script.replace("</", "<\\/"));
        snippet.push_str("</script>");
    }
    let lower = html.to_ascii_lowercase();
    if let Some(start) = lower.find("<head") {
        if let Some(end) = lower[start..].find('>') {
            let at = start + end + 1;
            let mut rewritten = String::with_capacity(html.len() + snippet.len());
            rewritten.push_str(&html[..at]);
            rewritten.push_str(&snippet);
            rewritten.push_str(&html[at..]);
            return rewritten;
        }
    }
    format!("{snippet}{html}")
}

fn is_html(content_type: &str, body: &[u8]) -> bool {
    if content_type.to_ascii_lowercase().contains("text/html") {
        return true;
    }
    let start = body
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(0);
    body.get(start).copied() == Some(b'<')
}

fn decode_cdp_body(value: &Value) -> Vec<u8> {
    let body = value["body"].as_str().unwrap_or("");
    if value["base64Encoded"].as_bool() == Some(true) {
        STANDARD
            .decode(body)
            .unwrap_or_else(|_| body.as_bytes().to_vec())
    } else {
        body.as_bytes().to_vec()
    }
}

fn header_pairs(value: &Value) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(entries) = value.as_array() {
        for entry in entries {
            let name = entry["name"].as_str().unwrap_or("");
            let header_value = entry["value"].as_str().unwrap_or("");
            if !name.is_empty() {
                headers.push((name.to_string(), header_value.to_string()));
            }
        }
    } else if let Some(object) = value.as_object() {
        for (name, header_value) in object {
            if let Some(header_value) = header_value.as_str() {
                headers.push((name.clone(), header_value.to_string()));
            }
        }
    }
    headers
}

fn header_entries(headers: &[(String, String)]) -> Value {
    Value::Array(
        headers
            .iter()
            .map(|(name, value)| json!({"name": name, "value": value}))
            .collect(),
    )
}

fn header_value(headers: &Value, name: &str) -> Option<String> {
    header_lookup(&header_pairs(headers), name).map(str::to_string)
}

fn header_lookup<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn upsert_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some((_, existing)) = headers
        .iter_mut()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
    {
        *existing = value.to_string();
    } else {
        headers.push((name.to_string(), value.to_string()));
    }
}

fn json_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn trace(message: impl std::fmt::Display) {
    if std::env::var_os("ZEN_CHROMIUM_DEBUG").is_some() {
        eprintln!("zen-chromium: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::inject_document;

    #[test]
    fn document_injection_lands_in_the_head() {
        let html = "<!doctype html><html><head><title>t</title></head><body><script src=\"/app.js\"></script></body></html>";
        let rewritten = inject_document(
            html,
            ".ad-box { display: none !important; }\n",
            "window.__scriptlet = true;",
        );
        let head = rewritten.find("<head>").unwrap();
        let injected = rewritten.find("data-zen-chromium").unwrap();
        let app = rewritten.find("/app.js").unwrap();
        assert!(head < injected && injected < app);
        assert!(rewritten.contains(".ad-box { display: none !important; }"));
        assert!(rewritten.contains("window.__scriptlet = true;"));
    }
}
