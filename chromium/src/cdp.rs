// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::browser::BrowserError;

pub struct Cdp {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    next_id: i64,
    queued: VecDeque<Incoming>,
}

pub enum Incoming {
    Result {
        id: i64,
        result: Value,
    },
    Failure {
        id: i64,
        message: String,
    },
    Event {
        method: String,
        params: Value,
        session_id: Option<String>,
    },
}

impl Cdp {
    pub fn connect(url: &str) -> Result<Self, BrowserError> {
        let (mut socket, _) = tungstenite::connect(url)
            .map_err(|error| BrowserError::Cdp(format!("devtools connection failed: {error}")))?;
        set_timeout(&mut socket, Duration::from_millis(400));
        Ok(Self {
            socket,
            next_id: 0,
            queued: VecDeque::new(),
        })
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, BrowserError> {
        self.call_in(method, params, None)
    }

    pub fn call_in(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, BrowserError> {
        let id = self.send(method, params, session_id)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if Instant::now() >= deadline {
                return Err(BrowserError::Timeout(format!(
                    "timed out waiting for DevTools {method}"
                )));
            }
            match self.read_one()? {
                Read::Idle => continue,
                Read::Message(Incoming::Result {
                    id: response_id,
                    result,
                }) if response_id == id => {
                    return Ok(result);
                }
                Read::Message(Incoming::Failure {
                    id: response_id,
                    message,
                }) if response_id == id => {
                    return Err(BrowserError::Cdp(format!("{method}: {message}")));
                }
                Read::Message(other) => self.queued.push_back(other),
            }
        }
    }

    pub fn poll(&mut self) -> Result<Option<Incoming>, BrowserError> {
        if let Some(message) = self.queued.pop_front() {
            return Ok(Some(message));
        }
        match self.read_one()? {
            Read::Idle => Ok(None),
            Read::Message(message) => Ok(Some(message)),
        }
    }

    pub(crate) fn send(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<i64, BrowserError> {
        self.next_id += 1;
        let id = self.next_id;
        let mut payload = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(session_id) = session_id {
            payload["sessionId"] = json!(session_id);
        }
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .map_err(|error| BrowserError::Cdp(format!("devtools write failed: {error}")))?;
        Ok(id)
    }

    fn read_one(&mut self) -> Result<Read, BrowserError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(Read::Message(parse_message(text.as_ref())?)),
            Ok(Message::Binary(bytes)) => {
                let text = String::from_utf8_lossy(&bytes);
                Ok(Read::Message(parse_message(&text)?))
            }
            Ok(Message::Ping(payload)) => {
                self.socket
                    .send(Message::Pong(payload))
                    .map_err(|error| BrowserError::Cdp(format!("devtools pong failed: {error}")))?;
                Ok(Read::Idle)
            }
            Ok(Message::Pong(_) | Message::Frame(_)) => Ok(Read::Idle),
            Ok(Message::Close(_)) => Err(BrowserError::Cdp(
                "Chromium closed the DevTools connection".to_string(),
            )),
            Err(error) if is_timeout(&error) => Ok(Read::Idle),
            Err(error) => Err(BrowserError::Cdp(format!("devtools read failed: {error}"))),
        }
    }
}

enum Read {
    Idle,
    Message(Incoming),
}

fn parse_message(text: &str) -> Result<Incoming, BrowserError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| BrowserError::Cdp(format!("devtools sent invalid JSON: {error}")))?;
    if let Some(method) = value.get("method").and_then(Value::as_str) {
        return Ok(Incoming::Event {
            method: method.to_string(),
            params: value.get("params").cloned().unwrap_or_else(|| json!({})),
            session_id: value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    let id = value.get("id").and_then(Value::as_i64).ok_or_else(|| {
        BrowserError::Cdp(format!("devtools message has no id or method: {text}"))
    })?;
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("devtools command failed")
            .to_string();
        return Ok(Incoming::Failure { id, message });
    }
    Ok(Incoming::Result {
        id,
        result: value.get("result").cloned().unwrap_or_else(|| json!({})),
    })
}

fn is_timeout(error: &tungstenite::Error) -> bool {
    match error {
        tungstenite::Error::Io(io_error) => {
            matches!(io_error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
        }
        _ => false,
    }
}

fn set_timeout(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>, timeout: Duration) {
    let stream = match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream,
        #[allow(unreachable_patterns)]
        _ => return,
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
}
