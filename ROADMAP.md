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

- [ ] Input modes: `enum Mode { Normal, SearchInput, ExportMenu, ExportPath }`; `handler.rs` dispatches by mode
- [ ] One-line status / input bar at the bottom of the screen
- [ ] Help tab (`5` / `?`) listing key bindings and query syntax
- [ ] `Tab` / `Shift-Tab` cycle tabs
- [ ] `body.rs` pipeline: `raw → decode → pretty → highlight → search-highlight → Vec<Line>`, shared by preview, export and pager (initially just today's JSON pretty printing)
- [ ] Preview cache key extended to cover the pipeline toggles
- [ ] Synthetic fixtures in `tests/fixtures/` (minified JS, JSON, XML, HTML, base64 text, base64 binary, missing optional fields, empty HAR)
- [ ] First unit tests (`#[cfg(test)]` modules)

### Phase 1 — Base64 decoding and XML/HTML pretty printing

- [ ] Add `base64`, `quick-xml`
- [ ] `x` toggles decoding of `encoding: "base64"` bodies
- [ ] Decoded binary (fonts, images) shown as a summary + hex dump, not raw bytes
- [ ] Decoded UTF-8 text flows on through pretty printing
- [ ] XML / HTML pretty printing via `quick-xml`; falls back to raw text when the markup doesn't parse
- [ ] `p` toggles pretty printing (raw view)
- [ ] Unit tests: decode text / binary / invalid base64, XML and HTML formatting, fallback

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
- [ ] Bodies are searched in decoded form (base64 included)
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
