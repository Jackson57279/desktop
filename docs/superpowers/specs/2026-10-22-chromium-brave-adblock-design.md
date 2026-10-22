<!--
   - This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/.
   -->

# Chromium port with Brave's adblock engine

## Goal

Run Chromium resource decisions through the same adblock engine Brave Browser ships, without changing Zen's Firefox chrome.

The window, themes, workspaces, and browser UI stay on the Firefox tree under `src/`. This port adds a sibling host, `chromium/`, that a Chromium embedder calls when a request starts, when response headers arrive, and when a document finishes loading.

## Why this shape

Zen's UI is Gecko chrome: XUL, `chrome://`, and XPCOM. Blink will not run those files. Copying them into a Chromium fork would be a UI rewrite, and a full `brave-core` checkout is a Chromium tree of tens of gigabytes. Neither belongs in this change.

Brave's blocker is not a Chromium patch. It is the `adblock` crate from [brave/adblock-rust](https://github.com/brave/adblock-rust) **0.13.3** (MPL-2.0). Brave desktop calls it from the browser process, then enforces the result on Chromium's network stack. This host implements that decision layer in-process so it can be tested with `cargo test` and linked into CEF or Chromium later.

What this is: the request, CSP, and cosmetic policy for the Chromium port.

What this is not: a replacement for `surfer build`, a new toolbar, or an in-page scriptlet interpreter.

## Engine

Crate `adblock` `=0.13.3`, default features off:

- `embedded-domain-resolver` so third-party checks use the public suffix list. Chromium's own PSL is not linked yet. When an embedder has `net::registry_controlled_domains`, it should pass a precomputed third-party bit instead of relying on this feature.
- `full-regex-handling` so `/regex/` rules compile.
- `css-validation` so cosmetic selectors are checked before they are returned.
- `debug-info` so a block can report the filter line that matched.

`single-thread` stays off. That feature removes `Send` and `Sync`. CEF calls `OnBeforeResourceLoad` on the IO thread and cosmetic injection on the UI thread, so `BraveAdblockHost` is `Send + Sync` and can be shared. One compiled engine is immutable aside from an internal regex cache.

Filter lists are compiled once with `FilterSet::new(true)` and `Engine::new_with_filter_set`. List updates replace the whole host. `serialize` / `from_dat` cache the compiled network and cosmetic rules. The cache does not contain scriptlet or redirect resources; callers pass those in again after a restore. The cache format is private to this crate version.

First-party requests are blocked when a rule matches. That is Brave's aggressive mode (`kBraveAdblockDefault1pBlocking`), not Shields' standard mode, which drops default-list blocks on first-party URLs. Zen's host is an adblocker, so a matching rule blocks.

## Chromium resource types

`ChromiumResourceType` uses Blink's names. The string passed into `Request::new` matches Brave's `AdBlockEngine::ResourceTypeToString`:

| Chromium type | adblock type string |
| --- | --- |
| `main_frame` | `main_frame` |
| `sub_frame` | `sub_frame` |
| `stylesheet` | `stylesheet` |
| `script` | `script` |
| `image`, `favicon` | `image` |
| `font` | `font` |
| `sub_resource` | `other` |
| `object` | `object` |
| `media` | `media` |
| `xhr` | `xhr` |
| `ping` | `ping` |
| `worker`, `shared_worker`, `service_worker`, `prefetch`, `csp_report`, `plugin`, `navigation_preload`, `other` | empty, which the engine treats as `other` |

`ws:` and `wss:` URLs are websockets inside `Request::new` even when the caller passes another type. Only `http`, `https`, `ws`, and `wss` are evaluated. Every other scheme is allowed and is not shown to the engine. A request the engine cannot parse is allowed. Failing open matches Brave's choice to skip schemes it does not understand.

## Network decision

`BraveAdblockHost::decide` and `decide_with_canonical_host` return `NetworkDecision`:

| Result | Meaning | Chromium enforcement |
| --- | --- | --- |
| `Allow { rewritten_url: None }` | No block | Continue the request |
| `Allow { rewritten_url: Some }` | `$removeparam` | Continue with the rewritten URL (307 inside Chromium) |
| `Cancel` | Subresource block, or a blocked websocket | `net::ERR_BLOCKED_BY_CLIENT` |
| `Stub { mime, body }` | Block whose rule supplied a redirect body | Answer 200 with that body and do not hit the network |
| `BlockNavigation` | Blocked `http(s)` main-frame document with no replacement body | Navigation throttle. Do not treat this as a silent subresource failure |

`$removeparam` is applied only for `GET`, `HEAD`, and `OPTIONS`. An empty method does not rewrite. A block suppresses the rewrite.

`$redirect` and `$redirect-rule` put a `data:` URL in `BlockerResult.redirect`. The host decodes that into `Stub` only when `should_block()` is true. `$redirect-rule` alone does not block and does not become a stub. A redirect body that is not a `data:` URL, or that fails to decode, becomes `Cancel` or `BlockNavigation` instead of a navigation to that string.

`$csp` is not a block. `content_security_policy` returns the directive string for `main_frame` and `sub_frame` `http(s)` documents. The embedder appends it to `Content-Security-Policy`. Other resource types return `None`. An `@@$csp` exception returns `None`.

### CNAME uncloaking

`decide_with_canonical_host` accepts the canonical hostname when the embedder has already resolved it. DNS stays in Chromium. The second check runs only when the first decision is not a block and the canonical host differs from the request host. A block on the canonical host is returned. A `$removeparam` result from the canonical URL is discarded. A rewrite from the original URL is kept when the canonical check does not block.

## Cosmetic filtering

`cosmetics` returns the URL-specific plan: hide selectors, procedural-action JSON, exceptions, the injected scriptlet source, `generichide`, and a stylesheet of `{ display: none !important; }` rules.

Generic `##.class` / `###id` rules are not in that plan. `cosmetics_for_dom` passes the bare class and id tokens actually present in the document to `hidden_class_id_selectors`, unless `generichide` is set. Cosmetic exceptions (`#@#`) are the exception set from the first call.

Procedural actions and `injected_script` are returned to the embedder. This host does not evaluate them. Brave runs that work in an isolated-world content script; the CEF shell should do the same.

## CEF contract

`cef` is the embedder surface. It does not link CEF.

- `on_before_resource_load` maps `Allow` to continue (with an optional URL override), `Cancel` to cancel, `Stub` to a synthetic body, and `BlockNavigation` to a navigation block.
- `on_headers_received` returns CSP directives to merge into the response.
- `on_load_end` returns the URL-specific cosmetic injection.
- `on_dom_tokens` returns the plan after class and id tokens are known.

`examples/embed.rs` is a headless call of that contract.

## Lists and configuration

Filter list text is not vendored. EasyList and EasyPrivacy are GPL-3.0-or-later and CC BY-SA. Brave's lists in `brave/adblock-lists` are MPL-2.0. Shipping those texts from this repository is a separate distribution choice.

`zen-chromium` reads list locations from repeatable `--list` arguments or from `ZEN_CHROMIUM_FILTER_LISTS` (comma-separated paths or `http(s)` URLs). `chromium/.env.example` shows the variable. A local `.env` is ignored by git. `ZEN_CHROMIUM_ENV_FILE` selects a specific env file. Existing environment variables win over the file.

`ZEN_CHROMIUM_RESOURCES` or `--resources` is a JSON array of adblock `Resource` values (scriptlets and redirect bodies). Each list file or response is capped at 64 MiB.

The CLI prints JSON. `decide` reports `allow`, `cancel`, `stub`, or `block_navigation` plus the matched rule when debug info is available. `cosmetics` prints the plan. `csp` prints the directives.

## Chromium session

`ChromiumSession` launches the Chromium binary (`google-chrome`, `chromium`, or `ZEN_CHROMIUM_BIN`) and attaches to its page DevTools socket. Chromium pauses every request on the Fetch domain, which is the browser-process enforcement point Brave uses: the decision is made outside the renderer, then Chromium continues, rewrites, cancels, or fulfills the response.

- `Cancel` calls `Fetch.failRequest` with `BlockedByClient`.
- `Allow` with a rewritten URL is answered with an internal 307 to that URL, which is how Brave applies `$removeparam`. The follow-up request is checked again. `$removeparam` is limited to `GET`, `HEAD`, and `OPTIONS` by the host.
- `Stub` calls `Fetch.fulfillRequest` with the redirect body and never opens a socket.
- `BlockNavigation` fulfills an HTML interstitial and does not request the blocked document.
- Document responses get an extra `Content-Security-Policy` header when `content_security_policy` returns directives. Multiple CSP headers are enforced together.
- Specific cosmetic CSS and `injected_script` are inserted immediately after `<head>` before the document is parsed, so scriptlets run in the page ahead of its own scripts.
- After `load`, class and id tokens are collected and generic cosmetic rules are inserted. Procedural actions are still returned to the caller and are not interpreted.

`zen-chromium browse URL` performs that navigation. It is headless unless `--headed` is set. `DevTools` `Fetch` and `XHR` both map to adblock `xhr`.

The Firefox chrome is still the UI for `npm run build`. This session is the Chromium port's browser: web content runs in Chromium, and every network decision goes through Brave's engine.

## Build

```bash
cargo test --manifest-path chromium/Cargo.toml --all-targets
cargo build --release --manifest-path chromium/Cargo.toml
```

`npm run test:chromium` and `npm run build:chromium` call those commands. `npm run build` remains the Firefox/Surfer build. Rust is pinned by the repository `rust-toolchain.toml` (1.95.0).

CI workflow `Chromium host` runs `cargo test --locked --all-targets` and `cargo clippy --all-targets -- -D warnings` when `chromium/` changes.

## UI boundary

Do not modify these trees for this port:

- `src/browser/**`
- `src/zen/**`
- `src/toolkit/themes/**`
- `locales/**`
- `configs/branding/**`
- `prefs/zen/**`

`README.md` gains a short pointer. `package.json` gains the two cargo scripts. No XUL, CSS, or chrome JS changes.

## Tests

`chromium/tests/brave_vectors.rs` covers third-party blocking, first-party `$third-party`, `@@` exceptions, `$important`, `$redirect`, `$redirect-rule`, `$csp`, `$removeparam` including the method gate, cosmetic hides, generic class selectors, `#@#` exceptions, `$generichide`, scriptlet injection, resource permissions, favicon-as-image, workers, websockets, unsupported schemes, main-frame versus subresource blocks, CNAME uncloaking, regex rules, DAT round-trip, and the CEF mapping. The CLI test drives the `zen-chromium` binary against a local list file.

`chromium/tests/chromium_session.rs` launches Chromium against a local site and checks the live browser: a third-party-style ad script is cancelled before it reaches the server, the page script still runs, `$removeparam` changes the document URL, specific and generic cosmetic rules hide elements, a scriptlet runs in the page, a CSP directive blocks inline and external scripts, a `$redirect` stub is executed in place of the network body, and a blocked navigation shows the interstitial without requesting the document.

## Later, not in this change

- CNAME lookup itself (the session consumes a hostname Chromium already resolved; `decide_with_canonical_host` is covered by the engine tests).
- Procedural cosmetic operators such as `:xpath` and `:remove`. The session returns them and does not run an in-page interpreter.
- Shields standard-mode first-party exceptions, regional list catalogs, and the component updater.
- Replacing `surfer.json`'s Firefox engine. The Zen window remains the Firefox chrome.
