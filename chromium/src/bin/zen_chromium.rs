// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use clap::{Parser, Subcommand};
use serde::Serialize;
use zen_chromium::filter_resources::Resource;
use zen_chromium::{
    read_filter_list, BraveAdblockHost, ChromiumResourceRequest, ChromiumResourceType,
    ChromiumSession, LaunchOptions, NetworkDecision, MAX_FILTER_LIST_BYTES,
};

const USER_AGENT: &str = "ZenChromium/1.0";

#[derive(Parser)]
#[command(
    name = "zen-chromium",
    version,
    about = "Apply Brave Browser's adblock engine to Chromium resource requests"
)]
struct Cli {
    /// Filter list file or http(s) URL. Repeat to add lists.
    /// Falls back to ZEN_CHROMIUM_FILTER_LISTS (comma-separated).
    #[arg(long, global = true, value_name = "FILE_OR_URL")]
    list: Vec<String>,

    /// JSON array of adblock Resource values. Falls back to ZEN_CHROMIUM_RESOURCES.
    #[arg(long, global = true, value_name = "FILE")]
    resources: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decide what Chromium should do with one request.
    Decide {
        #[arg(long)]
        url: String,
        /// Initiator URL. Omit for a third-party request with no initiator.
        #[arg(long)]
        source: Option<String>,
        /// main_frame, sub_frame, stylesheet, script, image, favicon, font,
        /// sub_resource, object, media, xhr, ping, worker, shared_worker,
        /// service_worker, prefetch, csp_report, plugin, navigation_preload, other.
        #[arg(long, value_name = "TYPE")]
        r#type: String,
        #[arg(long, default_value = "GET")]
        method: String,
        /// Canonical hostname from Chromium DNS. No scheme and no port.
        #[arg(long)]
        canonical_host: Option<String>,
    },
    /// Cosmetic plan for a document URL. Pass bare class and id tokens.
    Cosmetics {
        #[arg(long)]
        url: String,
        #[arg(long = "class")]
        classes: Vec<String>,
        #[arg(long = "id")]
        ids: Vec<String>,
    },
    /// CSP directives to merge into a document or subframe response.
    Csp {
        #[arg(long)]
        url: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long, default_value = "main_frame")]
        r#type: String,
        #[arg(long, default_value = "GET")]
        method: String,
    },
    /// Open a URL in Chromium with the adblock engine on every request.
    Browse {
        url: String,
        /// Show a window. The default is headless Chromium.
        #[arg(long)]
        headed: bool,
        #[arg(long)]
        binary: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    if let Err(error) = run() {
        eprintln!("zen-chromium: {error}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), CliError> {
    load_env()?;
    let cli = Cli::parse();
    let host = BraveAdblockHost::compile(load_lists(&cli.list)?, load_resources(cli.resources)?);
    match cli.command {
        Command::Decide {
            url,
            source,
            r#type,
            method,
            canonical_host,
        } => {
            let request = resource_request(url, source, &r#type, method)?;
            let decision = host.decide_with_canonical_host(&request, canonical_host.as_deref());
            print_json(&decide_report(&decision))?;
        }
        Command::Cosmetics { url, classes, ids } => {
            let classes: Vec<&str> = classes.iter().map(|value| bare_token(value, '.')).collect();
            let ids: Vec<&str> = ids.iter().map(|value| bare_token(value, '#')).collect();
            let plan = host.cosmetics_for_dom(&url, &classes, &ids);
            print_json(&plan)?;
        }
        Command::Csp {
            url,
            source,
            r#type,
            method,
        } => {
            let request = resource_request(url, source, &r#type, method)?;
            let directives = host.content_security_policy(&request);
            print_json(&CspReport { directives })?;
        }
        Command::Browse {
            url,
            headed,
            binary,
        } => {
            let mut options = if headed {
                LaunchOptions::headed()
            } else {
                LaunchOptions::headless()
            };
            if let Some(binary) = binary {
                options = options.binary(binary);
            }
            let mut session = ChromiumSession::launch(host, options)
                .map_err(|error| CliError::Message(error.to_string()))?;
            session
                .navigate(&url)
                .map_err(|error| CliError::Message(error.to_string()))?;
            let final_url = session
                .evaluate("location.href")
                .map_err(|error| CliError::Message(error.to_string()))?;
            let final_url = final_url.as_str().unwrap_or(&url).to_string();
            print_json(&BrowseReport {
                url: final_url,
                observed: session.observed().to_vec(),
            })?;
            if headed {
                session
                    .run_until_exit()
                    .map_err(|error| CliError::Message(error.to_string()))?;
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct BrowseReport {
    url: String,
    observed: Vec<zen_chromium::ObservedRequest>,
}

#[derive(Serialize)]
struct DecideReport {
    action: &'static str,
    rewritten_url: Option<String>,
    mime: Option<String>,
    body_base64: Option<String>,
    rule: Option<String>,
    exception: Option<String>,
    important: bool,
}

#[derive(Serialize)]
struct CspReport {
    directives: Option<String>,
}

fn decide_report(decision: &NetworkDecision) -> DecideReport {
    let info = decision.info();
    let (action, rewritten_url, mime, body_base64) = match decision {
        NetworkDecision::Allow { rewritten_url, .. } => {
            ("allow", rewritten_url.clone(), None, None)
        }
        NetworkDecision::Cancel { .. } => ("cancel", None, None, None),
        NetworkDecision::Stub { mime, body, .. } => (
            "stub",
            None,
            Some(mime.clone()),
            Some(STANDARD.encode(body)),
        ),
        NetworkDecision::BlockNavigation { .. } => ("block_navigation", None, None, None),
    };
    DecideReport {
        action,
        rewritten_url,
        mime,
        body_base64,
        rule: info.rule.clone(),
        exception: info.exception.clone(),
        important: info.important,
    }
}

fn resource_request(
    url: String,
    source: Option<String>,
    resource_type: &str,
    method: String,
) -> Result<ChromiumResourceRequest, CliError> {
    let resource_type = resource_type
        .parse::<ChromiumResourceType>()
        .map_err(|error| CliError::Message(error.to_string()))?;
    Ok(ChromiumResourceRequest {
        url,
        initiator: source,
        resource_type,
        method,
    })
}

fn bare_token(value: &str, prefix: char) -> &str {
    value.strip_prefix(prefix).unwrap_or(value)
}

fn print_json(value: &impl Serialize) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Message(error.to_string()))?;
    println!("{json}");
    Ok(())
}

fn load_lists(cli_lists: &[String]) -> Result<Vec<String>, CliError> {
    let specs = if cli_lists.is_empty() {
        std::env::var("ZEN_CHROMIUM_FILTER_LISTS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    } else {
        cli_lists.to_vec()
    };
    if specs.is_empty() {
        return Err(CliError::Message(
            "pass --list or set ZEN_CHROMIUM_FILTER_LISTS".to_string(),
        ));
    }
    specs.iter().map(|spec| read_spec(spec)).collect()
}

fn read_spec(spec: &str) -> Result<String, CliError> {
    if spec.starts_with("https://") || spec.starts_with("http://") {
        return fetch_list(spec);
    }
    read_filter_list(Path::new(spec)).map_err(|error| CliError::Message(error.to_string()))
}

fn fetch_list(url: &str) -> Result<String, CliError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(5)
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| CliError::Message(format!("failed to fetch {url}: {error}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_FILTER_LIST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::Message(format!("failed to read {url}: {error}")))?;
    if bytes.len() > MAX_FILTER_LIST_BYTES {
        return Err(CliError::Message(format!(
            "{url} exceeds {MAX_FILTER_LIST_BYTES} bytes"
        )));
    }
    String::from_utf8(bytes).map_err(|_| CliError::Message(format!("{url} is not valid UTF-8")))
}

fn load_resources(cli_path: Option<PathBuf>) -> Result<Vec<Resource>, CliError> {
    let path = match cli_path {
        Some(path) => path,
        None => match std::env::var("ZEN_CHROMIUM_RESOURCES") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => return Ok(Vec::new()),
        },
    };
    let text = fs::read_to_string(&path).map_err(|error| {
        CliError::Message(format!("failed to read {}: {error}", path.display()))
    })?;
    serde_json::from_str(&text)
        .map_err(|error| CliError::Message(format!("invalid resources JSON: {error}")))
}

fn load_env() -> Result<(), CliError> {
    if let Ok(path) = std::env::var("ZEN_CHROMIUM_ENV_FILE") {
        load_env_file(Path::new(&path), true)?;
        return Ok(());
    }
    load_env_file(Path::new("chromium/.env"), false)?;
    load_env_file(Path::new(".env"), false)?;
    Ok(())
}

fn load_env_file(path: &Path, required: bool) -> Result<(), CliError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(()),
        Err(error) => {
            return Err(CliError::Message(format!(
                "failed to read {}: {error}",
                path.display()
            )));
        }
    };
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            return Err(CliError::Message(format!(
                "{}:{} is not KEY=VALUE",
                path.display(),
                index + 1
            )));
        };
        let key = key.trim();
        if key.is_empty()
            || key
                .chars()
                .any(|character| !(character.is_ascii_alphanumeric() || character == '_'))
        {
            return Err(CliError::Message(format!(
                "{}:{} has an invalid variable name",
                path.display(),
                index + 1
            )));
        }
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, trim_env_value(value.trim()));
        }
    }
    Ok(())
}

fn trim_env_value(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[derive(Debug)]
enum CliError {
    Message(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}
