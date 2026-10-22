// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::process::Command;

fn rules_file(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zen-chromium-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rules.txt");
    std::fs::write(&path, body).unwrap();
    path
}

fn zen_chromium() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zen-chromium"))
}

#[test]
fn decide_reports_a_cancel() {
    let list = rules_file("decide", "||ads.example^$script\n");
    let output = zen_chromium()
        .args([
            "decide",
            "--url",
            "https://ads.example/ad.js",
            "--source",
            "https://news.example/",
            "--type",
            "script",
            "--list",
        ])
        .arg(&list)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["action"], "cancel");
    assert_eq!(report["rule"], "||ads.example^$script");
    let _ = std::fs::remove_file(list);
}

#[test]
fn missing_list_fails() {
    let output = zen_chromium()
        .env_remove("ZEN_CHROMIUM_FILTER_LISTS")
        .env_remove("ZEN_CHROMIUM_ENV_FILE")
        .args([
            "decide",
            "--url",
            "https://example.com/",
            "--type",
            "script",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ZEN_CHROMIUM_FILTER_LISTS"), "{stderr}");
}
