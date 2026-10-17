<div align="center">

# harview

HTTP Archive (HAR) Viewer on the terminal written in Rust

*This tool is still under development. Please note that specifications are subject to change without notice.*

</div>

> [!NOTE]
> This project is based on [**harview** by sheepla](https://github.com/sheepla/harview), licensed under MIT.
> The original was an early work in progress and could not open most real-world HAR files.
> This repository continues it with the fixes and changes listed in [What's different from the original](#whats-different-from-the-original),
> and will add new features on top.


## About

**harview** is a viewer of HAR files that runs on the terminal. You can easily view HAR files exported from a web browser without opening the browser.

The goal of this tool is not to provide in-depth analysis capabilities like DevTool, but to provide the ability to browse HAR files with a lightweight UI like an easy-to-use pager for those familiar with the command line interface.

## What's different from the original

- **Real-world HAR files load.** Fractional `pageTimings` (as written by Chrome and Firefox) no longer fail to parse, and fields that some exporters leave out (`cache`, `timings`, `postData.text`, `redirectURL`, sizes, headers, cookies, ...) are optional.
- **All preview tabs work.** Headers, Cookies, Request and Response are fully rendered (the original hit a `todo!()`); JSON bodies are pretty-printed.
- **Scrollable preview** that accounts for line wrapping, so long single-line bodies (minified JS/CSS/HTML, JWTs) can be read to the end. The current position is shown in the preview border.
- **Responsiveness:** table rows and preview content are built once and cached instead of being rebuilt every frame, and the screen is not redrawn on idle ticks.
- **Usability fixes:** table scroll position is kept, mouse capture is disabled so terminal text selection works, unknown sizes (`-1`) are shown as `-`, and the terminal is always restored on exit, even after an error.
- **No crash on an empty HAR** when jumping to the last entry.

Planned features and their progress are tracked in [ROADMAP.md](ROADMAP.md).

## Usage

### Export HAR files from Browsers

Open your web browser's DevTools and export the HAR file.
It supports the HAR format, which can be exported in the following browsers:

- Firefox
- Chromium based browsers

**Firefox**:
    
1. Open DevTool (type `F12` or `Ctrl-Shift-I`)
2. Open *Network* tab and select *Save All as HAR* from context menu

> [Network request list - Firefox Source Docs](https://firefox-source-docs.mozilla.org/devtools-user/network_monitor/request_list/index.html)

**Chromium**:

1. Open DevTool (type `F12` or `Ctrl-Shift-I`)
2. Click *Export HAR* below the tab bar

> [Network features reference - Chrome DevTools](https://developer.chrome.com/docs/devtools/network/reference)

### Use *harview* TUI

To use harview, specify the path of the HAR file as the first argument. Once the HAR file is loaded, entries in the HTTP log will appear in the table.

```sh
harview example.com.har
```
TUI can be controlled with the following keys:

| Key | Action |
|-----|---------|
| `k` / `j`, `Up` / `Down` | Move the focus up / down |
| `u` /  `d` | Move the focus up / down more fast |
| `g` / `G` | Jump to the first / last entry |
| `1` - `4` | Switch preview widget tab |
| `5` / `?` | Show the Help tab |
| `Tab` / `Shift-Tab` | Next / previous tab |
| `p` | Toggle pretty printing (JSON, XML, HTML) |
| `x` | Toggle decoding: base64 bodies (binary is shown as a hex dump), and URL-encoded / base64 form fields and query parameters such as a `SAMLResponse` |
| `J` / `K`, `PgDn` / `PgUp` | Scroll the preview |
| `/` | Filter entries with a query (see below) |
| `f` | Find in the preview, without filtering the table |
| `n` / `N` | Next / previous match in the preview |
| `Esc` | Clear find, then the filter |
| `q` or `Ctrl-C` | Quit application |

#### Search queries

Press `/`, type a query and press `Enter`. Terms are combined with AND, and `!` negates a term.
Regexes are case-insensitive unless they contain an uppercase letter. Bodies, form fields and query
parameters are searched in decoded form (URL-encoding and base64), whether or not `x` is on.

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

## Installation

Clone this repository then run `cargo install`

```sh
git clone <this-repository-url> harview
cd harview
cargo install --path .
```

> [!WARNING]
> HAR files contain the full request and response data of a browsing session, including cookies,
> authorization headers and tokens. Treat them as secrets. The `data/` directory and `*.har` files
> are git-ignored so local captures are never committed by accident.

## References

- [HAR (file format) - Wikipedia](https://en.wikipedia.org/wiki/HAR_%28file_format%29)
- [HTTP Archive (HAR) Format Specifications - W3C](https://w3c.github.io/web-performance/specs/HAR/Overview.html)

## License

MIT. See [LICENSE](LICENSE). The original copyright notice from sheepla's harview is retained as required by the license.

## Thanks

- [sheepla/harview](https://github.com/sheepla/harview) - The original project this is based on.
- [ratatui](https://ratatui.rs/) - This tool was built with ratatui, a TUI library for Rust. Thank you for the amazing library and its ecosystem!
