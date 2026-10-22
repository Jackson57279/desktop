// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Zen's sidebar, spaces, and vertical tabs for the Chromium window.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::browser::BrowserError;

const SIDEBAR_HTML: &str = include_str!("../chrome/sidebar.html");
const SIDEBAR_CSS: &str = include_str!("../chrome/sidebar.css");
const SIDEBAR_JS: &str = include_str!("../chrome/sidebar.js");
const COLORS: [&str; 6] = [
    "#7aa2f7", "#f07178", "#c3e88d", "#ffcb6b", "#c792ea", "#89ddff",
];

#[derive(Clone, Serialize)]
struct Space {
    id: String,
    name: String,
    color: String,
}

#[derive(Clone, Serialize)]
struct Tab {
    id: String,
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "spaceId")]
    space_id: String,
    title: String,
    url: String,
    pinned: bool,
    active: bool,
}

#[derive(Clone, Serialize)]
struct Model {
    compact: bool,
    #[serde(rename = "rightSide")]
    right_side: bool,
    #[serde(rename = "activeSpaceId")]
    active_space_id: String,
    #[serde(rename = "sidebarTargetId")]
    sidebar_target_id: String,
    spaces: Vec<Space>,
    tabs: Vec<Tab>,
    #[serde(skip)]
    next_id: u64,
}

pub enum ChromeCommand {
    NewTab,
    Activate { tab_id: String },
    Close { tab_id: String },
    Navigate { url: String },
    Pin { tab_id: String, pinned: bool },
    NewSpace { name: String },
    ActivateSpace { space_id: String },
    RenameSpace { space_id: String, name: String },
    ToggleCompact,
    ToggleSide,
}

struct HubInner {
    state: Mutex<Model>,
    commands: Mutex<VecDeque<ChromeCommand>>,
    stop: AtomicBool,
}

pub struct ZenHub {
    inner: Arc<HubInner>,
    origin: String,
    thread: Option<JoinHandle<()>>,
}

impl ZenHub {
    pub fn start(content_target_id: String) -> Result<Self, BrowserError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let origin = format!("http://127.0.0.1:{port}");
        let inner = Arc::new(HubInner {
            state: Mutex::new(Model::new(content_target_id)),
            commands: Mutex::new(VecDeque::new()),
            stop: AtomicBool::new(false),
        });
        let accepted = Arc::clone(&inner);
        let thread = thread::spawn(move || serve(listener, accepted));
        Ok(Self {
            inner,
            origin,
            thread: Some(thread),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn pop_commands(&self) -> Vec<ChromeCommand> {
        let mut commands = self.inner.commands.lock().expect("zen chrome commands");
        commands.drain(..).collect()
    }

    pub fn note_page(&self, target_id: &str, url: &str, title: &str) {
        let mut state = self.inner.state.lock().expect("zen chrome state");
        if let Some(tab) = state.tabs.iter_mut().find(|tab| tab.target_id == target_id) {
            tab.url = url.to_string();
            if !title.is_empty() {
                tab.title = title.to_string();
            }
        }
    }

    pub fn set_sidebar_target(&self, target_id: String) {
        self.inner
            .state
            .lock()
            .expect("zen chrome state")
            .sidebar_target_id = target_id;
    }

    pub fn add_tab(&self, target_id: String, url: String) -> String {
        let mut state = self.inner.state.lock().expect("zen chrome state");
        let space_id = state.active_space_id.clone();
        for tab in &mut state.tabs {
            if tab.space_id == space_id {
                tab.active = false;
            }
        }
        let id = state.fresh_id("tab");
        state.tabs.push(Tab {
            id: id.clone(),
            target_id,
            space_id,
            title: "New Tab".to_string(),
            url,
            pinned: false,
            active: true,
        });
        id
    }

    pub fn tab_target(&self, tab_id: &str) -> Option<String> {
        self.inner
            .state
            .lock()
            .expect("zen chrome state")
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.target_id.clone())
    }

    pub fn active_target(&self) -> Option<String> {
        self.inner
            .state
            .lock()
            .expect("zen chrome state")
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.target_id.clone())
    }

    pub fn set_active_tab(&self, tab_id: &str) {
        let mut state = self.inner.state.lock().expect("zen chrome state");
        let Some(space_id) = state
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.space_id.clone())
        else {
            return;
        };
        state.active_space_id = space_id.clone();
        for tab in &mut state.tabs {
            tab.active = tab.id == tab_id;
        }
    }

    pub fn close_tab(&self, tab_id: &str) -> Option<String> {
        let mut state = self.inner.state.lock().expect("zen chrome state");
        let index = state.tabs.iter().position(|tab| tab.id == tab_id)?;
        let removed = state.tabs.remove(index);
        if removed.active {
            if let Some(next) = state
                .tabs
                .iter_mut()
                .rev()
                .find(|tab| tab.space_id == removed.space_id)
            {
                next.active = true;
                return Some(next.target_id.clone());
            }
        }
        None
    }

    pub fn set_active_url(&self, url: String) {
        let mut state = self.inner.state.lock().expect("zen chrome state");
        if let Some(tab) = state.tabs.iter_mut().find(|tab| tab.active) {
            tab.url = url;
            if tab.url == "about:blank" {
                tab.title = "New Tab".to_string();
            }
        }
    }
}

impl Drop for ZenHub {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.origin.trim_start_matches("http://"));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Model {
    fn new(content_target_id: String) -> Self {
        let space_id = "space-1".to_string();
        Self {
            compact: false,
            right_side: false,
            active_space_id: space_id.clone(),
            sidebar_target_id: String::new(),
            spaces: vec![Space {
                id: space_id.clone(),
                name: "Space".to_string(),
                color: COLORS[0].to_string(),
            }],
            tabs: vec![Tab {
                id: "tab-1".to_string(),
                target_id: content_target_id,
                space_id,
                title: "New Tab".to_string(),
                url: "about:blank".to_string(),
                pinned: false,
                active: true,
            }],
            next_id: 2,
        }
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }
}

fn serve(listener: TcpListener, hub: Arc<HubInner>) {
    let _ = listener.set_nonblocking(false);
    while !hub.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let hub = Arc::clone(&hub);
                thread::spawn(move || handle(stream, hub));
            }
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream, hub: Arc<HubInner>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(size) => {
                buffer.extend_from_slice(&chunk[..size]);
                if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines = text.lines();
    let Some(request) = lines.next() else {
        return;
    };
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");
    let length = text
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    let header_end = text
        .find("\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(text.len());
    let mut body = text[header_end.min(text.len())..].to_string();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(size) => body.push_str(&String::from_utf8_lossy(&chunk[..size])),
        }
    }
    let payload = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
    if method == "POST" {
        if let Some(command) = command_for(path, &payload) {
            if !apply_local(&hub, &command) {
                hub.commands
                    .lock()
                    .expect("zen chrome commands")
                    .push_back(command);
            }
        }
        respond(&mut stream, "200 OK", "application/json", b"{}");
        return;
    }
    let (content_type, bytes) = match path {
        "/" | "/sidebar.html" => ("text/html; charset=utf-8", SIDEBAR_HTML.as_bytes()),
        "/sidebar.css" => ("text/css; charset=utf-8", SIDEBAR_CSS.as_bytes()),
        "/sidebar.js" => ("text/javascript; charset=utf-8", SIDEBAR_JS.as_bytes()),
        "/api/state" => {
            let json = serde_json::to_vec(&*hub.state.lock().expect("zen chrome state"))
                .unwrap_or_else(|_| b"{}".to_vec());
            respond(&mut stream, "200 OK", "application/json", &json);
            return;
        }
        _ => {
            respond(&mut stream, "404 Not Found", "text/plain", b"missing");
            return;
        }
    };
    respond(&mut stream, "200 OK", content_type, bytes);
}

fn apply_local(hub: &HubInner, command: &ChromeCommand) -> bool {
    let mut state = hub.state.lock().expect("zen chrome state");
    match command {
        ChromeCommand::NewSpace { name } => {
            let id = state.fresh_id("space");
            let color = COLORS[state.spaces.len() % COLORS.len()].to_string();
            state.spaces.push(Space {
                id: id.clone(),
                name: if name.trim().is_empty() {
                    "Space".to_string()
                } else {
                    name.trim().to_string()
                },
                color,
            });
            state.active_space_id = id;
            true
        }
        ChromeCommand::ActivateSpace { space_id } => {
            if state.spaces.iter().any(|space| &space.id == space_id) {
                state.active_space_id = space_id.clone();
            }
            true
        }
        ChromeCommand::RenameSpace { space_id, name } => {
            if let Some(space) = state.spaces.iter_mut().find(|space| &space.id == space_id) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    space.name = trimmed.to_string();
                }
            }
            true
        }
        ChromeCommand::ToggleCompact => {
            state.compact = !state.compact;
            true
        }
        ChromeCommand::ToggleSide => {
            state.right_side = !state.right_side;
            true
        }
        ChromeCommand::Pin { tab_id, pinned } => {
            if let Some(tab) = state.tabs.iter_mut().find(|tab| &tab.id == tab_id) {
                tab.pinned = *pinned;
            }
            true
        }
        _ => false,
    }
}

fn command_for(path: &str, payload: &Value) -> Option<ChromeCommand> {
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    match path {
        "/api/new-tab" => Some(ChromeCommand::NewTab),
        "/api/activate" => Some(ChromeCommand::Activate {
            tab_id: text("tabId"),
        }),
        "/api/close" => Some(ChromeCommand::Close {
            tab_id: text("tabId"),
        }),
        "/api/navigate" => Some(ChromeCommand::Navigate { url: text("url") }),
        "/api/pin" => Some(ChromeCommand::Pin {
            tab_id: text("tabId"),
            pinned: payload
                .get("pinned")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }),
        "/api/new-space" => Some(ChromeCommand::NewSpace { name: text("name") }),
        "/api/activate-space" => Some(ChromeCommand::ActivateSpace {
            space_id: text("spaceId"),
        }),
        "/api/rename-space" => Some(ChromeCommand::RenameSpace {
            space_id: text("spaceId"),
            name: text("name"),
        }),
        "/api/compact" => Some(ChromeCommand::ToggleCompact),
        "/api/right-side" => Some(ChromeCommand::ToggleSide),
        _ => None,
    }
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

pub fn normalize_location(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "about:blank".to_string();
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("about:")
    {
        return trimmed.to_string();
    }
    if trimmed.contains(' ') || !trimmed.contains('.') {
        let query: String =
            percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC)
                .collect();
        return format!("https://duckduckgo.com/?q={query}");
    }
    format!("https://{trimmed}")
}
