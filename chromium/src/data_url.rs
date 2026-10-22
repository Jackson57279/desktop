// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;

/// Decode a `data:` URL into a MIME type and body.
///
/// Brave's `$redirect` result is a data URL whose body is the replacement
/// response, not a URL the browser should navigate to.
pub fn parse_data_url(value: &str) -> Option<(String, Vec<u8>)> {
    let rest = value.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mut parts = meta.split(';').filter(|part| !part.is_empty());
    let mime = parts.next().unwrap_or("text/plain");
    let mime = if mime.eq_ignore_ascii_case("base64") {
        "text/plain"
    } else {
        mime
    };
    let base64 = meta
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"));
    let body = if base64 {
        STANDARD.decode(data.trim()).ok()?
    } else {
        percent_encoding::percent_decode_str(data).collect()
    };
    let mime = if mime.is_empty() {
        "text/plain".to_string()
    } else {
        mime.to_string()
    };
    Some((mime, body))
}

#[cfg(test)]
mod tests {
    use super::parse_data_url;

    #[test]
    fn decodes_base64_javascript() {
        let (mime, body) = parse_data_url("data:application/javascript;base64,bm9vcA==").unwrap();
        assert_eq!(mime, "application/javascript");
        assert_eq!(body, b"noop");
    }

    #[test]
    fn decodes_percent_encoded_text() {
        let (mime, body) = parse_data_url("data:text/plain,hello%20world").unwrap();
        assert_eq!(mime, "text/plain");
        assert_eq!(body, b"hello world");
    }

    #[test]
    fn rejects_non_data_urls() {
        assert!(parse_data_url("https://example.com/noop.js").is_none());
    }
}
