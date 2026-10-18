<div align="center">

# harview

A fast HTTP Archive (HAR) viewer for the terminal, written in Rust

*Under active development: key bindings and behaviour may still change.*

</div>

> [!NOTE]
> This project is based on [**harview** by sheepla](https://github.com/sheepla/harview), licensed under MIT.
> The original was an early work in progress and could not open most real-world HAR files.
> This repository fixes that and adds search, decoding, syntax highlighting and export; see
> [What's different from the original](#whats-different-from-the-original).

## About

**harview** lets you browse a HAR file exported from a browser's DevTools without opening the
browser: a table of every request on top, and the selected request's headers, cookies, request
body and response body below. It's built for working from the command line, over SSH, or with
captures too large for DevTools to handle comfortably.

It isn't meant to replace the DevTools Network panel, but it covers what you usually need when
someone sends you a HAR file to debug: find the request, read what was sent and received, decode
it, and replay it.

## Features

- **Split view:** request table and a scrollable preview with Headers, Cookies, Request, Response
  and Help tabs. `z` gives the preview the full screen.
- **Search and filter:** a small query language (`method:POST status:4xx`, `host:api !type:image`,
  `body:"invalid credentials"`) that filters the table and highlights matches; find-in-preview with
  `n` / `N` to jump between matches.
- **Decoding:** base64 bodies (binary shown as a hex dump), and URL-encoded, base64 and
  deflate-compressed form fields and query parameters, so a SAML `SAMLResponse` or `SAMLRequest`
  reads as XML. Searches always look at the decoded text.
- **Pretty printing and syntax highlighting** for JSON, XML, HTML, JavaScript and CSS.
- **Export** a request or response body, or the full request / response as raw HTTP, to a file or
  the clipboard (OSC 52, works over SSH).
- **Copy as curl:** a bash script that replays the request, with credentials in variables you can
  override, or the plain `curl` command for the clipboard.
- **Handles real captures:** tens of megabytes, Chrome and Firefox quirks (`null` timings,
  missing fields), and multi-megabyte single-line bodies.

## Installation

harview is a single self-contained binary built from source with Cargo. It runs on Linux, macOS
and WSL; it has been developed and tested on Linux / WSL.

### Prerequisites

- **Rust 1.88 or newer** with Cargo. Install it with [rustup](https://rustup.rs/) (or your usual
  tool manager), then check with `cargo --version`. Update an older toolchain with `rustup update`.
- **A C linker** (`cc`), which Rust needs on Linux to link the binary: e.g. `build-essential` on
  Debian / Ubuntu, Xcode Command Line Tools on macOS. No other system libraries are needed: syntax
  highlighting and decompression are pure Rust.

### Install

```sh
git clone git@github.com:mikew79/harview.git   # or https://github.com/mikew79/harview.git
cd harview
cargo install --path .
```

This builds an optimised binary and copies it to `~/.cargo/bin/harview`. Make sure `~/.cargo/bin` is
on your `PATH` (rustup sets this up), then run `harview --version` to check.

To update, pull and install again (an install from `--path` always rebuilds and replaces the
binary); to remove it, uninstall:

```sh
git pull && cargo install --path .   # update
cargo uninstall harview              # remove
```

### Build and run without installing

```sh
cargo build --release                   # binary at ./target/release/harview
./target/release/harview capture.har

cargo run --release -- capture.har      # or build and run in one step
```

A debug build (`cargo build`, `cargo run -- capture.har`) compiles faster but is noticeably slower
with large HAR files and syntax highlighting; use `--release` for real captures.

### Run the tests

```sh
cargo test
```

Tests use the synthetic HAR files in [`tests/fixtures/`](tests/fixtures/), never real captures.

## Usage

### Export a HAR file from your browser

**Chrome, Edge and other Chromium-based browsers:**

1. Open DevTools (`F12` or `Ctrl-Shift-I`) and select the *Network* tab
2. Reproduce the problem, then click *Export HAR* (the download icon) in the Network toolbar

> [Network features reference - Chrome DevTools](https://developer.chrome.com/docs/devtools/network/reference)

Recent Chromium versions export a *sanitized* HAR by default, without `Cookie`, `Set-Cookie` and
`Authorization` headers. That's usually what you want when sharing a capture; if you need those
headers, enable *Allow to generate HAR with sensitive data* in the DevTools settings and choose
*Export HAR (with sensitive data)*.

**Firefox:**

1. Open DevTools (`F12` or `Ctrl-Shift-I`) and select the *Network* tab
2. Reproduce the problem, then right-click a request and choose *Save All As HAR*

> [Network request list - Firefox Source Docs](https://firefox-source-docs.mozilla.org/devtools-user/network_monitor/request_list/index.html)

> [!WARNING]
> HAR files contain the full request and response data of a browsing session, which can include
> cookies, authorization headers, tokens, passwords and SAML assertions. Treat them as secrets.
> In this repository the `data/` directory and `*.har` files are git-ignored so local captures are
> never committed by accident.

### Open it in harview

Pass the path of the HAR file:

```sh
harview capture.har
harview --help      # usage
harview --version
```

The screen is split into the request table (status, method, domain, file name, content type and
size), the preview of the selected request, and a status bar showing the mode, active toggles,
your position in the list and the latest message. Press `?` at any time for the built-in key
reference.

### Keys

| Key | Action |
|-----|--------|
| `j` / `k`, `Down` / `Up` | Select the next / previous request |
| `d` / `u` | Move down / up by several requests |
| `g` / `G` | First / last request |
| `1` - `4` | Preview tab: Headers, Cookies, Request, Response |
| `5` / `?` | Help tab (all keys and the query language) |
| `Tab` / `Shift-Tab` | Next / previous tab |
| `J` / `K`, `PgDn` / `PgUp` | Scroll the preview |
| `z` | Full-screen preview / restore the split |
| `p` | Toggle pretty printing (JSON, XML, HTML) |
| `s` | Toggle syntax highlighting (JSON / XML up to 200KB, JavaScript / CSS / HTML up to 32KB) |
| `x` | Toggle decoding: base64 bodies, and URL-encoded / base64 / deflate-compressed form fields and query parameters |
| `/` | Filter requests with a [search query](#search-queries) |
| `f` | Find in the preview, without filtering the table |
| `n` / `N` | Next / previous match in the preview |
| `e` | [Export](#export) the selected request or response |
| `Esc` | Clear the find, then the filter; cancel an input or popup |
| `q`, `Ctrl-C` | Quit |

### Search queries

Press `/`, type a query and press `Enter`; the table shows only matching requests and the matches
are highlighted. Terms are combined with AND, and `!` negates a term. Regexes are case-insensitive
unless they contain an uppercase letter. Bodies, form fields and query parameters are searched in
decoded form (URL-encoding, base64 and deflate), whether or not `x` is on. An empty query shows
everything again.

| Term | Matches |
|------|---------|
| `token` | Regex anywhere: URL, headers, parameters, bodies |
| `url:` / `host:` | Regex on the full URL / host name |
| `header:` | Regex on request and response headers (`Name: value`) |
| `param:` | Regex on decoded query parameters and form fields (`name=value`) |
| `body:` | Regex on decoded request and response bodies |
| `type:` | Regex on the response content type |
| `method:POST` | Request method |
| `status:404`, `status:4xx`, `status:>=500` | Status code, class or comparison |
| `size:>100k`, `size:<=2mb` | Response size (decimal units) |
| `dur:>500ms`, `dur:<2s` | Total request time |
| `"a b"` | Quote terms that contain spaces |

For example: `method:POST status:4xx`, `host:api !type:image`, `body:"invalid credentials"`.

`f` searches only the preview of the selected request, with the same regex rules, and `n` / `N`
step through the matches.

#### Checking SAML logins

`SAMLResponse` form fields (and `SAMLRequest` query parameters) are searched as decoded XML, so you
can check which claims a login carried:

```text
/param:claims/emailaddress\b                          logins whose assertion has an emailaddress claim
/param:SAMLResponse !param:claims/emailaddress\b      logins without it
/param:NameID                                         assertions with a NameID
```

Then open the Request tab (`3`), press `x` to show the assertion decoded and pretty printed, and use
`f` with `n` / `N` to jump to the matching `<Attribute>` line.

### Export

Press `e` to export the selected entry, then choose what to export:

| Key | Exports |
|-----|---------|
| `1` | Request body |
| `2` | Response body |
| `3` | Full request: request line, headers and body as raw HTTP |
| `4` | Full response: status line, headers and body as raw HTTP |
| `5` | A `curl` command / bash script that replays the request |

`d` switches between *decoded* (as shown in the preview: base64 decoded, pretty printed, and form
fields such as a `SAMLResponse` decoded) and *exactly as stored in the HAR*. `Enter` saves to a
file, by default under `exports/` in the current directory (you can edit the name; an existing file
is only replaced if you press `Enter` a second time); `c` copies to the clipboard. Exported files
are readable by you only.

#### Copy as curl

`5` turns the request into `curl`, to replay or tweak it from a shell. `Enter` saves an executable
bash script; credential headers (cookies, `Authorization`, anything named like a token or credential)
become variables at its top that you can override from the environment, and any arguments are passed
on to curl:

```sh
./exports/0012-api.example.com-login-request.sh -i                  # replay, showing response headers
AUTHORIZATION='Bearer new-token' ./exports/0012-...-request.sh      # with a fresh token
```

`c` copies the plain command instead, as the browser's *Copy as cURL* does. The request is replayed
exactly as captured, whatever `d` is set to; bodies over 32KB are saved in a file next to the script.

> [!CAUTION]
> Scripts and commands contain the captured credentials and request bodies (e.g. passwords or SAML
> assertions). Treat them like the HAR itself before sharing.

#### Clipboard

The clipboard uses the OSC 52 terminal escape sequence, so it also works over SSH. Your terminal
must allow it (Windows Terminal, iTerm2, kitty, WezTerm and others do); inside tmux, add
`set -g set-clipboard on` to `~/.tmux.conf`. Binary content and anything over 100KB can only be
saved to a file.

## What's different from the original

Fixes to the original code:

- **Real-world HAR files load.** Fractional and `null` page timings (as written by Chrome and
  Firefox) no longer fail to parse, and fields that some exporters leave out (`cache`, `timings`,
  `postData.text`, `redirectURL`, sizes, headers, cookies, ...) are optional.
- **All preview tabs work.** Headers, Cookies, Request and Response are fully rendered (the
  original hit a `todo!()`).
- **Scrollable preview** that accounts for line wrapping, so long single-line bodies (minified
  JS/CSS/HTML, JWTs) can be read to the end. The current position is shown in the preview border.
- **Responsiveness:** table rows and preview content are built once and cached instead of being
  rebuilt every frame, only the visible part of a long body is rendered, and the screen is not
  redrawn on idle ticks.
- **Usability:** the table scroll position is kept, mouse capture is disabled so terminal text
  selection works, unknown sizes (`-1`) are shown as `-`, the terminal is always restored on exit
  (even after an error), and an empty HAR no longer crashes.

New features are listed under [Features](#features). Planned work and the decisions behind each
phase are tracked in [ROADMAP.md](ROADMAP.md).

## References

- [HAR (file format) - Wikipedia](https://en.wikipedia.org/wiki/HAR_%28file_format%29)
- [HTTP Archive (HAR) Format Specifications - W3C](https://w3c.github.io/web-performance/specs/HAR/Overview.html)

## License

MIT. See [LICENSE](LICENSE). The original copyright notice from sheepla's harview is retained as required by the license.

## Thanks

- [sheepla/harview](https://github.com/sheepla/harview) - The original project this is based on.
- [ratatui](https://ratatui.rs/) - This tool was built with ratatui, a TUI library for Rust. Thank you for the amazing library and its ecosystem!
