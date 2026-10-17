# Roadmap

Planned extensions to harview. Each phase ends with a commit, a warning-free build, and a manual check
against the synthetic fixtures in `tests/fixtures/`. Tick items off in the same commit that completes them.

> [!IMPORTANT]
> Develop and test only against synthetic HAR fixtures. Real captures (`data/`, `*.har`) contain cookies,
> tokens and other secrets: never commit them, paste them into issues, or print their contents in logs.

## Decisions

| Topic | Decision |
|---|---|
| New crates | `regex`, `base64`, `quick-xml`, `syntect` (with `default-fancy`, pure Rust, no C dependency). Temp files use `std`, not `tempfile`. |
| Search behaviour | An active search **filters** the table to matching entries and highlights matches in both panes. `Esc` clears it. |
| Export targets | File in `./exports/` (git-ignored) **and** clipboard via OSC 52. |
| External viewer | `bat` → `batcat` → `$PAGER` → `less`. |
| Syntax highlighting limit | Skipped for bodies over 200KB; use `b` to view those in the pager. |

## Key bindings

Existing keys keep their meaning. New keys are marked ✚.

| Key | Action |
|---|---|
| `j` / `k`, `Down` / `Up` | Move focus down / up |
| `d` / `u` | Move focus faster |
| `g` / `G` | First / last entry |
| `J` / `K`, `PgDn` / `PgUp` | Scroll preview |
| `1` - `4` | Switch tab |
| ✚ `5` / `?` | Help tab |
| ✚ `Tab` / `Shift-Tab` | Next / previous tab |
| ✚ `/` | Search / filter (see query language below) |
| ✚ `n` / `N` | Next / previous match in preview |
| ✚ `Esc` | Clear search, close popup |
| ✚ `s` | Toggle syntax highlighting |
| ✚ `p` | Toggle pretty printing |
| ✚ `x` | Toggle base64 decoding |
| ✚ `b` | Open current body in external pager |
| ✚ `e` | Export menu |
| ✚ `+` / `-` | Resize split |
| ✚ `z` | Maximise / restore preview |
| `q`, `Ctrl-C` | Quit |

## Phases

### Phase 0 — Groundwork

Structure that later phases plug into. No user-visible change apart from the Help tab and status bar.

- [x] Input modes: `enum Mode` with `handler.rs` dispatching by mode. Only `Normal` exists so far; `SearchInput`, `ExportMenu` and `ExportPath` are added by the phases that use them
- [x] One-line status bar at the bottom of the screen (mode, file name, entry position, hints)
- [x] Help tab (`5` / `?`) listing key bindings, generated from `handler::KEY_HELP` (query syntax added in phase 2)
- [x] `Tab` / `Shift-Tab` cycle tabs
- [x] `body.rs` pipeline (`raw → decode → pretty → …`), UI-free so export and pager can reuse it; `ui.rs` turns its `Processed` result into lines. Currently JSON pretty printing only
- [x] Preview cache key (`PreviewKey`) covers entry, tab and `BodyOptions`
- [x] Synthetic fixtures in `tests/fixtures/` (`sample.har`: JSON, form post, minified JS, XML, HTML, CSS, base64 text, base64 PNG, missing body; `minimal.har`; `empty.har`). `.gitignore` excludes `*.har` except this folder
- [x] First unit tests (`#[cfg(test)]` modules in `app`, `body`, `handler`, `har`) — run with `cargo test`

### Phase 1 — Base64 decoding and XML/HTML pretty printing

- [x] Add `base64`, `quick-xml`
- [x] `x` toggles decoding of `encoding: "base64"` bodies (off by default; the placeholder hints at `x`)
- [x] Decoded binary (fonts, images) shown as a size summary + hex dump of the first 4KB, not raw bytes. Text containing NUL bytes counts as binary
- [x] Decoded UTF-8 text flows on through pretty printing
- [x] XML / HTML pretty printing via `quick-xml`, picked by MIME type (`BodyKind`); falls back to raw text when the markup doesn't parse. HTML is best effort: void elements, `script`/`style`/`pre`/`textarea` content and mismatched end tags are handled. JSON is still tried for unknown types
- [x] `p` toggles pretty printing (raw view)
- [x] Status bar shows the `pretty` / `decode` toggles (green = on)
- [x] Unit tests: decode text / binary / invalid base64, XML and HTML formatting, fallback, MIME detection, hex dump
- [x] `x` also decodes request data the HAR doesn't mark as encoded: form fields (from `params`, or parsed from a form body) and query parameters are URL-decoded, then base64-decoded (standard or URL-safe, padding optional) when that yields readable text, and pretty printed — e.g. a SAML `SAMLResponse`. Decoded fields are tagged `[base64]`; opaque tokens (binary results, values under 16 chars) are left alone. Whole bodies that look like base64 are decoded the same way
- [x] Fix: accept `null` where Chrome writes it (e.g. `pageTimings.onLoad` when the page hadn't finished loading, plus optional sizes, strings and lists); new `tests/fixtures/nulls.har`

### Phase 2 — Search and filter

The riskiest phase: introduces a filtered view, which changes how the selection is tracked everywhere.

- [ ] Add `regex`
- [ ] `App.visible: Vec<usize>`; `index` becomes a position in it; table, preview and export look up entries through it (land this on its own first)
- [ ] `/` opens the input bar, `Enter` applies, `Esc` clears; regex compiled on `Enter`, not per keystroke
- [ ] Query parser (terms are ANDed, `!` negates a term):
  ```
  token                         regex matched anywhere (URL, headers, bodies)
  url:/api/v\d+  host:example   field regex
  header:authorization          header name or value
  body:"error"                  request or response body
  status:4xx  status:>=500      exact, class, or comparison
  method:POST
  size:>100k  dur:>500ms        size / duration with units
  ```
- [ ] Bodies, form fields and query parameters are searched in decoded form (base64 included)
- [ ] Matches highlighted in table rows and preview lines
- [ ] `n` / `N` jump between matches in the preview (scrolls to the match)
- [ ] Status bar shows the active query and `matched / total`
- [ ] Unit tests: parser (each field, units, comparisons, negation, invalid regex), matching

### Phase 3 — Syntax highlighting

- [ ] Add `syntect` (`default-fancy`)
- [ ] JSON, HTML, XML, JavaScript, CSS; language picked from MIME type
- [ ] `s` toggles highlighting
- [ ] Skipped above 200KB with a status bar hint to use `b`
- [ ] syntect styles converted to ratatui spans; search highlighting layered on top
- [ ] Cache the wrapped-line count per width so redrawing huge bodies stays fast
- [ ] Unit tests: language selection, size cut-off

### Phase 4 — External pager

- [ ] `b` writes the current body (after decoding / pretty printing) to a temp file with owner-only permissions (`0600`) and an extension matching the MIME type
- [ ] Pager resolution: `bat` → `batcat` → `$PAGER` → `less`
- [ ] Stop the event-reader task before launching (otherwise it steals the pager's keystrokes), restore the terminal, run the pager, then re-enter the TUI and restart the task
- [ ] Temp file deleted afterwards, including when the pager fails
- [ ] Error shown in the status bar if no pager is found

### Phase 5 — Export

- [ ] `e` opens a popup menu:
  - request body
  - response body
  - full request (request line + headers + body, raw HTTP text)
  - full response (status line + headers + body)
- [ ] Option: decoded body or exactly as stored in the HAR
- [ ] Destination: file or clipboard
- [ ] File: filename prompt with a default like `0012-api.example.com-users.json`, written to `./exports/`; add `exports/` to `.gitignore`
- [ ] Clipboard: OSC 52. Over the size cap (~100KB) copy nothing and explain in the status bar. Needs `set -g set-clipboard on` inside tmux
- [ ] Unit tests: raw HTTP formatting, default filename generation

### Phase 6 — Split-pane extras and docs

- [ ] `+` / `-` resize the table / preview split
- [ ] `z` maximises / restores the preview
- [ ] README: key bindings, query language, export, pager

## Backlog

Ideas that came up during development. Not scheduled into a phase yet.

- [ ] **Find in preview.** Search *within* the content currently shown in the preview pane, including decoded and pretty printed content such as a base64 `SAMLResponse` expanded into a SAML assertion. Separate from the phase 2 search, which filters the entries table: this one jumps between matches inside a single entry's preview (e.g. `Ctrl-F` or `/` while the preview is focused, with `n` / `N`), so the text being searched is exactly what's on screen after decoding. Could share phase 2's regex compilation and match highlighting
- [ ] **Inflate deflate-compressed values**, e.g. a SAML HTTP-Redirect `SAMLRequest` in a query string (raw DEFLATE, then base64, then URL-encoding). Would need the `flate2` crate
