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
| Syntax highlighting limit | Per language, to keep the UI responsive: JSON / XML up to 200KB, JavaScript / CSS / HTML up to 32KB (syntect's JS-family grammars cost ~3µs per byte, so 200KB of JS blocked for ~0.6s). Larger bodies are shown plain with a note; use `b` to view them in the pager. |

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
| ✚ `f` | Find in preview (doesn't filter the table) |
| ✚ `n` / `N` | Next / previous match in preview |
| ✚ `Esc` | Clear find, then search; close popup |
| ✚ `s` | Toggle syntax highlighting |
| ✚ `p` | Toggle pretty printing |
| ✚ `x` | Toggle base64 decoding |
| ✚ `b` | Open current body in external pager |
| ✚ `e` | Export menu |
| ✚ `z` | Full-screen preview / restore |
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

Delivered in three steps, each building and passing tests before the next:
**(1)** filtered view, **(2)** `/` filter and query language, **(3)** highlighting, `n` / `N` and find in preview.

- [x] Add `regex`
- [x] `App.visible: Vec<usize>`; `index` becomes a position in it; table, preview and export look up entries through it (land this on its own first)
- [x] `/` opens the input bar, `Enter` applies, `Esc` clears; regex compiled on `Enter`, not per keystroke; invalid regex keeps the input open with the error shown
- [x] Smart case: case-insensitive unless the pattern contains an uppercase letter
- [x] Query parser (terms are ANDed, `!` negates a term):
  ```
  token                         regex matched anywhere (URL, headers, bodies)
  url:/api/v\d+  host:example   field regex
  header:authorization          header name or value
  body:"error"                  request or response body
  status:4xx  status:>=500      exact, class, or comparison
  method:POST
  size:>100k  dur:>500ms        size / duration with units
  ```
  Also `param:` (decoded query parameters and form fields) and `type:` (response content type). Sizes use decimal units, like the table
- [x] Bodies, form fields and query parameters are searched in decoded form (base64 included), whatever the `x` setting. Decoded search text is built lazily on the first search and cached
- [x] When the filter matched decoded content but `x` is off (so nothing is highlighted), the status bar says so
- [x] Matches highlighted in table rows (domain, path) and preview lines; the filter's positive regex terms are combined into one highlight pattern
- [x] `n` / `N` jump between individual matches in the preview (several per line count separately), scrolling to the wrapped row the match is on, so matches deep inside a minified line are reachable
- [x] After filtering, the preview switches to the first tab with a match; toggling `x` / `p` re-runs this, so decoding reveals the match. The status bar shows `match 2/5`, which other tab has matches, or that `x` is needed
- [x] **Find in preview** (`f`): searches only what the preview currently shows (after decoding / pretty printing, e.g. a decoded SAML assertion) without filtering the table; shares highlighting and `n` / `N`. `Esc` clears find first, then the filter. Moved here from the backlog
- [x] Status bar shows the active query and `matched / total`
- [x] Unit tests: parser (each field, units, comparisons, negation, quoting, invalid input), matching against decoded content, smart case, highlighting (span splitting, multibyte), wrapped-row calculation, input editing, filter / find state, key handling

### Phase 3 — Syntax highlighting

- [x] Add `syntect` (`default-fancy`, pure Rust; it also pulls in `flate2`, which the backlog's deflate item needs)
- [x] JSON, HTML, XML, JavaScript, CSS; language picked from MIME type, else sniffed from the content (also used for decoded multi-line form fields such as a SAML assertion). On by default; theme `base16-ocean.dark`, foreground colours only
- [x] `s` toggles highlighting; the status bar shows `syntax`
- [x] Size limits per language (see Decisions); skipped bodies show `(syntax highlighting skipped: JavaScript body is over 32KB)`. The hint to use `b` is added in phase 4
- [x] syntect styles converted to ratatui spans; search highlighting layered on top
- [x] Wrapped layout cached per preview and width (it used to be recounted every frame), and only the visible lines are copied for rendering
- [x] Long (≥10KB) ASCII lines, i.e. minified code, are wrapped at exactly the preview width instead of at word boundaries, so their layout is arithmetic and only visible rows are cut out: on a 19MB HAR, moving through entries on the Response tab went from ~20ms to ~12ms per entry; jumping to matches in such lines is exact
- [x] Finding which other tab has a match renders the tabs without pretty printing or highlighting
- [x] Unit tests: language detection and sniffing, bundled syntaxes, per-language limits, span conversion, wrapped layout, visible window slicing, hard wrapping

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

### Phase 6 — Full-screen preview

- [ ] `z` toggles the preview (bottom half) to full screen and back; the table is hidden while maximised. Help tab and README list the key

## Backlog

Ideas that came up during development. Not scheduled into a phase yet.

- [ ] **Faster moves with a filter active.** On a 19MB HAR, moving between filtered entries still costs ~25-30ms, now mostly from running the search regex over every line of multi-megabyte bodies when the preview is rebuilt. Could highlight only the visible window and count matches lazily
- [ ] **Inflate deflate-compressed values**, e.g. a SAML HTTP-Redirect `SAMLRequest` in a query string (raw DEFLATE, then base64, then URL-encoding). Would need the `flate2` crate (already in the dependency tree via `syntect`)
