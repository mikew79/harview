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
| External viewer | Won't do (see the end of this file): export (phase 5) covers the need without handing the terminal to another program. |
| Next phases | Agreed order: 7 deflate decoding (`SAMLRequest`), 8 copy as curl / bash script, 9 faster moves with a filter active. The SAML inspector stays in the backlog until it's scheduled. |
| Syntax highlighting limit | Per language, to keep the UI responsive: JSON / XML up to 200KB, JavaScript / CSS / HTML up to 32KB (syntect's JS-family grammars cost ~3µs per byte, so 200KB of JS blocked for ~0.6s). Larger bodies are shown plain with a note; export them to view elsewhere. |

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
- [x] Size limits per language (see Decisions); skipped bodies show `(syntax highlighting skipped: JavaScript body is over 32KB)`. Export (phase 5) is the way to view them elsewhere
- [x] syntect styles converted to ratatui spans; search highlighting layered on top
- [x] Wrapped layout cached per preview and width (it used to be recounted every frame), and only the visible lines are copied for rendering
- [x] Long (≥10KB) ASCII lines, i.e. minified code, are wrapped at exactly the preview width instead of at word boundaries, so their layout is arithmetic and only visible rows are cut out: on a 19MB HAR, moving through entries on the Response tab went from ~20ms to ~12ms per entry; jumping to matches in such lines is exact
- [x] Finding which other tab has a match renders the tabs without pretty printing or highlighting
- [x] Unit tests: language detection and sniffing, bundled syntaxes, per-language limits, span conversion, wrapped layout, visible window slicing, hard wrapping

### Phase 5 — Export

- [x] `e` opens a popup menu (`1`-`4` or `j` / `k`; the default follows the current tab):
  - request body
  - response body
  - full request (request line + headers + body, raw HTTP text)
  - full response (status line + headers + body)
- [x] `d`: decoded (as previewed: base64 decoded, pretty printed when `p` is on) or exactly as stored in the HAR. A decoded form body is exported as its decoded fields, e.g. a `SAMLResponse` as its XML assertion. Decoded binary (images, fonts) is written as the real bytes
- [x] Destination: file (`Enter` / `f`) or clipboard (`c`)
- [x] File: filename prompt prefilled like `exports/0012-api.example.com-users-response-body.json` (extension from the MIME type, `.http` for full exports, `.b64` for undecoded base64), editable; parent directories created; owner-only permissions (`0600`); an existing file is only replaced when the same path is submitted twice. `exports/` is git-ignored. `Esc` returns to the menu
- [x] Clipboard: OSC 52, written to the terminal. Binary content and anything over 100KB is refused with an explanation. Needs `set -g set-clipboard on` inside tmux
- [x] Empty or missing bodies are reported instead of exporting nothing; start lines skip fields the HAR left empty
- [x] Unit tests: raw HTTP formatting, decoded vs stored bodies, decoded form fields, binary, default file names and sanitising, OSC 52 encoding and limits, file writing (no overwrite, `0600`), menu state and key handling

### Phase 6 — Full-screen preview

- [x] `z` toggles the preview (bottom half) to full screen and back; the table is hidden while maximised and keeps its selection and scroll position, and `j` / `k` still move between entries (the status bar shows which). The preview border shows `z: full screen` / `z: restore`. Help tab and README list the key

### Phase 7 — Deflate decoding (SAML `SAMLRequest`)

The SAML HTTP-Redirect binding sends the AuthnRequest in the URL as `SAMLRequest`: the XML is raw-DEFLATE compressed, then base64- and URL-encoded. Phase 1 decodes the IdP's `SAMLResponse` (a form post, base64 only), but a `SAMLRequest` currently decodes to compressed binary and is left encoded, so the request that started the login can't be read or searched. Found in the real captures: 2 `SAMLRequest` query parameters in each of `anglogold.har` and `chromewebdata.har`, all inflating to XML.

- [x] Add `flate2` as a direct dependency (already in the build via `syntect`, same version, pure-Rust backend)
- [x] In the decode pipeline, when base64 yields binary, try raw-DEFLATE inflating it; keep the result only if it's readable text, the same rule as base64. This matters: in the captures two `sso_nonce` values also "inflated", to binary noise, and now correctly stay encoded
- [x] Only a complete stream that uses up exactly all of the input counts: testing showed a truncated stream otherwise inflates to partial output (`<a><b>x<`) with no error, and damaged data can stop early on a valid-looking end (`<a8`). Inflated output is capped at 10MB against decompression bombs
- [x] Applies to query parameters, form fields and whole bodies; inflated XML is pretty printed and highlighted, and found by `/` and `f` like any decoded content (e.g. `/param:AuthnRequest`), and exported decoded
- [x] Decoded fields tagged `[deflate+base64]` instead of `[base64]`
- [x] Synthetic `SAMLRequest` redirect fixture (entry 12 of `sample.har`); unit tests for inflating, the readable-text rule, binary output, truncated and damaged streams, the size cap, and filtering. Verified on the real captures: all 4 `SAMLRequest`s inflate to a `<samlp:AuthnRequest>`

### Phase 8 — Copy as curl / bash script

Turn the selected request into a `curl` command, like the browser DevTools' "Copy as cURL", to replay or tweak it from a shell. A fifth export target, so it gets the clipboard and file destinations for free.

- [ ] **Bash script as the main output**: an executable `.sh` file (e.g. `exports/0012-api.example.com-login-request.sh`) with `#!/usr/bin/env bash` and `set -euo pipefail`, the curl command one option per line, owner-only executable permissions (`0700`). Secrets become variables at the top (`TOKEN=...`, `COOKIE=...`) so they're easy to swap or read from the environment; a binary or large body sits next to the script as a file it references
- [ ] The plain command is also available for the clipboard
- [ ] Shell quoting: single-quote every argument (`'` becomes `'\''`); DevTools also offers Windows `cmd` / PowerShell variants, which quote differently
- [ ] Method: `-X` only when curl wouldn't infer it (e.g. not for GET, or POST with a body)
- [ ] Headers: drop HTTP/2 pseudo-headers (`:authority`, `:method`, ...) and `Content-Length`; turn `Accept-Encoding` into `--compressed`; optionally `--http2` from the HTTP version
- [ ] Body: `--data-raw` for text; binary or very large bodies need `--data-binary @file`, which pairs with exporting the body to a file; `multipart/form-data` as `--form` fields
- [ ] Secrets: the command carries cookies and `Authorization` headers; consider an option to leave them out (or replace them with placeholders) when sharing
- [ ] Unit tests: quoting (incl. quotes, newlines, non-ASCII), method inference, header filtering, body variants, secrets as variables, script layout and permissions

### Phase 9 — Faster moves with a filter active

- [ ] On a 19MB HAR, moving between filtered entries still costs ~25-30ms, mostly from running the search regex over every line of multi-megabyte bodies when the preview is rebuilt. Highlight only the visible window and count matches lazily; measure before and after on the real captures

## Backlog

Ideas that came up during development. Not scheduled into a phase yet.

- [ ] **SAML inspector.** Searching decoded SAML already works: `/param:claims/emailaddress\b` finds logins whose `SAMLResponse` carries that claim (and confirmed one was missing in a real capture, the cause of a bug), and `x` + `n` jump to the `<Attribute>` line. But it's text matching, and the values have to be read from the XML. A SAML-aware view would add:
  - a `claim:` query term matching attribute *names* only, e.g. `/claim:emailaddress`
  - a claims summary above the decoded XML: NameID and its format, Issuer, Audience, validity window (`NotBefore` / `NotOnOrAfter`), signed / encrypted, then each attribute name → values
  - works for `SAMLRequest` too once phase 7 is in

## Won't do

- **External pager** (was phase 4, `b` to open a body in `bat` / `less`). Export covers the need (view the file in any tool, copy via the clipboard, clean text either way), and handing the terminal to another program would be the most fragile code in the project. Kept here so it isn't proposed again without new reasons
