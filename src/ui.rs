use crate::app::{App, InputKind, MatchJump, Mode, PreviewCache, TabBarState};
use crate::body::{self, Body, BodyOptions, Processed};
use crate::handler::KEY_HELP;
use crate::har::{Entry, Header};
use ratatui::{prelude::*, widgets::*};
use regex::Regex;

const MATCH_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Yellow);

/// Rows of context kept above a match when jumping to it
const MATCH_CONTEXT_ROWS: usize = 2;

pub fn render(app: &mut App, frame: &mut Frame) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .split(frame.size());

    render_table(app, main_layout[0], frame.buffer_mut());
    render_preview(app, main_layout[1], frame.buffer_mut());
    // After the preview, which resolves match jumps and may set a message
    render_status_bar(app, main_layout[2], frame.buffer_mut());
}

pub fn render_status_bar(app: &App, area: Rect, buf: &mut Buffer) {
    match app.mode {
        Mode::Input(kind) => render_input_bar(app, kind, area, buf),
        Mode::Normal => render_normal_status(app, area, buf),
    }
}

fn message_span(app: &App) -> Option<Span<'static>> {
    app.message.as_ref().map(|message| {
        let color = if message.error {
            Color::LightRed
        } else {
            Color::LightYellow
        };
        Span::styled(message.text.clone(), Style::default().fg(color))
    })
}

fn render_input_bar(app: &App, kind: InputKind, area: Rect, buf: &mut Buffer) {
    let prompt = match kind {
        InputKind::Filter => Span::styled(" / ", Style::default().fg(Color::Black).bg(Color::LightYellow).bold()),
        InputKind::Find => Span::styled(" find ", Style::default().fg(Color::Black).bg(Color::LightMagenta).bold()),
    };
    // The terminal cursor is hidden, so draw one
    let text = app.input.text();
    let split = text
        .char_indices()
        .nth(app.input.cursor())
        .map_or(text.len(), |(offset, _)| offset);
    let (before, rest) = text.split_at(split);
    let mut rest = rest.chars();
    let under_cursor = rest.next().map_or(" ".to_string(), String::from);

    let left = Line::from(vec![
        prompt,
        Span::raw(" "),
        Span::raw(before.to_string()),
        Span::styled(under_cursor, Style::default().reversed()),
        Span::raw(rest.as_str().to_string()),
    ]);
    let right = Line::from(message_span(app).unwrap_or_else(|| {
        Span::styled(" Enter: apply  Esc: cancel ", Style::default().fg(Color::DarkGray))
    }))
    .right_aligned();

    Widget::render(right, area, buf);
    Widget::render(left, area, buf);
}

fn render_normal_status(app: &App, area: Rect, buf: &mut Buffer) {
    let total = app.max_index();
    let position = if total == 0 { 0 } else { app.get_index() + 1 };
    let count = if total == app.total_entries() {
        format!("{}/{}", position, total)
    } else {
        format!("{}/{} of {}", position, total, app.total_entries())
    };

    let mut left = vec![
        Span::styled(" NORMAL ", Style::default().fg(Color::Black).bg(Color::LightCyan).bold()),
        Span::raw(" "),
    ];
    match message_span(app) {
        Some(message) => left.push(message),
        None => {
            left.push(Span::raw(app.source.clone()));
            if let Some((text, _)) = &app.filter {
                left.push(Span::styled(format!("  /{}", text), Style::default().fg(Color::LightYellow)));
            }
            if let Some((text, _)) = &app.find {
                left.push(Span::styled(format!("  find:{}", text), Style::default().fg(Color::LightMagenta)));
            }
        }
    }

    let flag = |label: &'static str, on: bool| {
        let style = if on {
            Style::default().fg(Color::LightGreen)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Span::styled(label, style)
    };
    let mut right = Vec::new();
    if app.highlight().is_some() {
        right.push(match_summary(app));
    }
    right.extend([
        flag("pretty ", app.body_options.pretty),
        flag("decode ", app.body_options.decode),
        Span::raw(format!(" {} ", count)),
        Span::styled(" ?: help  q: quit ", Style::default().fg(Color::DarkGray)),
    ]);

    Widget::render(Line::from(right).right_aligned(), area, buf);
    Widget::render(Line::from(left), area, buf);
}

/// "match 2/5", or why nothing is highlighted on this tab
fn match_summary(app: &App) -> Span<'static> {
    let count = app
        .preview_cache
        .as_ref()
        .map_or(0, |cache| cache.matches.len());
    let elsewhere = app
        .preview_cache
        .as_ref()
        .and_then(|cache| cache.match_elsewhere);
    if count > 0 {
        let current = app
            .match_cursor
            .map_or("-".to_string(), |cursor| (cursor + 1).to_string());
        return Span::styled(
            format!("match {}/{}  ", current, count),
            Style::default().fg(Color::Yellow),
        );
    }
    let hint = match elsewhere {
        Some(tab) => format!("no matches here, see {}  ", tab.title().trim()),
        // The filter searches decoded content, but the preview only shows it decoded with `x`
        None if !app.body_options.decode => "no matches on this tab (x: decode)  ".to_string(),
        None => "no matches on this tab  ".to_string(),
    };
    Span::styled(hint, Style::default().fg(Color::DarkGray))
}

pub fn render_table(app: &mut App, area: Rect, buf: &mut Buffer) {
    let headers = Row::new(vec![
        Cell::from("Status"),
        Cell::from("Method"),
        Cell::from("Domain"),
        Cell::from("FileName"),
        Cell::from("ContentType"),
        Cell::from("     Size  "),
    ])
    .style(Style::default().bold().underlined());

    let widths = [
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Fill(1),
        Constraint::Fill(2),
        Constraint::Length(12),
        Constraint::Length(12),
    ];

    let highlight = app.table_highlight();
    let rows = app
        .visible()
        .iter()
        .map(|&entry| app.table_items[entry].to_table_row(highlight));

    let mut block = Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    if app.visible().is_empty() && app.total_entries() > 0 {
        block = block.title_bottom(Line::from(" no entries match the filter (Esc: clear) ").centered());
    }

    let table = Table::new(rows, widths)
        .header(headers)
        .highlight_style(Style::default().reversed())
        .block(block);

    StatefulWidget::render(table, area, buf, &mut app.table_state);
}

/// Rebuilds the cached preview lines (with highlighting) when anything they depend on changed
fn ensure_preview_cache(app: &mut App) {
    let key = app.preview_key();
    if app.preview_cache.as_ref().map(|cache| cache.key) == Some(key) {
        return;
    }
    let lines = preview_lines(app, app.tabbar_state);
    let (lines, matches) = match app.highlight() {
        Some(regex) => highlight_lines(lines, regex),
        None => (lines, Vec::new()),
    };
    // Only computed on a cache rebuild: it renders the other tabs, which is costly for big bodies
    let match_elsewhere = if matches.is_empty() {
        first_other_tab_with_match(app)
    } else {
        None
    };
    app.preview_cache = Some(PreviewCache {
        key,
        lines,
        matches,
        match_elsewhere,
    });
    app.match_cursor = None;
}

pub fn render_preview(app: &mut App, area: Rect, buf: &mut Buffer) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .split(area);
    let block = Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(layout[1]);

    ensure_preview_cache(app);
    if let Some(jump) = app.match_jump.take() {
        resolve_match_jump(app, jump, inner.width);
    }

    Widget::render(tabbar(app.tabbar_state), layout[0], buf);

    let lines = app
        .preview_cache
        .as_ref()
        .map(|cache| cache.lines.clone())
        .unwrap_or_default();
    let content = Paragraph::new(lines).wrap(Wrap { trim: false });

    // Count rows after wrapping, so a single long line (e.g. a minified body) is still scrollable.
    // Clamp here too so scrolling back up responds immediately after overshooting.
    let total = content.line_count(inner.width);
    let max_scroll = total
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    app.preview_scroll = app.preview_scroll.min(max_scroll);

    let position = format!(
        " {}/{} | J/K PgUp/PgDn: scroll ",
        (app.preview_scroll as usize + inner.height as usize).min(total),
        total
    );
    let content = content
        .scroll((app.preview_scroll, 0))
        .block(block.title_bottom(Line::from(position).right_aligned()));

    Widget::render(content, layout[1], buf);
}

fn resolve_match_jump(app: &mut App, jump: MatchJump, width: u16) {
    let mut count = app
        .preview_cache
        .as_ref()
        .map_or(0, |cache| cache.matches.len());

    // After filtering, show the first tab that actually contains a match
    if count == 0 && jump == (MatchJump::First { any_tab: true }) {
        let elsewhere = app
            .preview_cache
            .as_ref()
            .and_then(|cache| cache.match_elsewhere);
        if let Some(tab) = elsewhere {
            app.tabbar_state = tab;
            app.preview_scroll = 0;
            ensure_preview_cache(app);
            count = app
                .preview_cache
                .as_ref()
                .map_or(0, |cache| cache.matches.len());
        }
    }
    if count == 0 {
        if !matches!(jump, MatchJump::First { .. }) {
            app.info("no matches on this tab");
        }
        return;
    }

    let cursor = match jump {
        MatchJump::First { .. } => 0,
        MatchJump::Next => app.match_cursor.map_or(0, |cursor| (cursor + 1) % count),
        MatchJump::Prev => app
            .match_cursor
            .map_or(count - 1, |cursor| (cursor + count - 1) % count),
    };
    app.match_cursor = Some(cursor);

    if let Some(cache) = &app.preview_cache {
        let (line, match_end) = cache.matches[cursor];
        let row = match_row(&cache.lines, line, match_end, width);
        app.preview_scroll = row
            .saturating_sub(MATCH_CONTEXT_ROWS)
            .min(u16::MAX as usize) as u16;
    }
}

fn first_other_tab_with_match(app: &App) -> Option<TabBarState> {
    let regex = app.highlight()?;
    TabBarState::ALL
        .into_iter()
        .filter(|tab| *tab != TabBarState::Help && *tab != app.tabbar_state)
        .find(|tab| {
            preview_lines(app, *tab)
                .iter()
                .any(|line| regex.is_match(&line_text(line)))
        })
}

fn wrapped_height(line: Line<'static>, width: u16) -> usize {
    Paragraph::new(line)
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}

/// Row (after wrapping at `width`) on which logical line `index` starts
fn wrapped_row(lines: &[Line<'static>], index: usize, width: u16) -> usize {
    lines[..index.min(lines.len())]
        .iter()
        .map(|line| wrapped_height(line.clone(), width))
        .sum()
}

/// Row on which a match ending at byte `match_end` of line `index` is shown. Wrapping the text
/// up to the match end approximates the full line's wrapping (it can be one row early), which
/// the context rows kept above a match absorb
fn match_row(lines: &[Line<'static>], index: usize, match_end: usize, width: u16) -> usize {
    let text = lines.get(index).map(line_text).unwrap_or_default();
    let prefix = text.get(..match_end).unwrap_or(&text).to_string();
    wrapped_row(lines, index, width) + wrapped_height(Line::from(prefix), width) - 1
}

fn line_text(line: &Line) -> String {
    line.spans.iter().map(|span| span.content.as_ref()).collect()
}

/// Highlights every match of `regex` in `line`; also returns the byte offset of each match end
pub fn highlight_line(line: Line<'static>, regex: &Regex) -> (Line<'static>, Vec<usize>) {
    let text = line_text(&line);
    let ranges: Vec<(usize, usize)> = regex
        .find_iter(&text)
        .filter(|found| !found.is_empty())
        .map(|found| (found.start(), found.end()))
        .collect();
    if ranges.is_empty() {
        return (line, Vec::new());
    }
    let match_ends = ranges.iter().map(|&(_, end)| end).collect();

    let Line {
        spans: old_spans,
        style,
        alignment,
    } = line;
    let mut spans = Vec::new();
    let mut offset = 0;
    let mut range = 0;
    for span in old_spans {
        let content = span.content.as_ref();
        let (start, end) = (offset, offset + content.len());
        let mut position = start;
        while position < end {
            while ranges.get(range).is_some_and(|&(_, match_end)| match_end <= position) {
                range += 1;
            }
            let (piece_end, matched) = match ranges.get(range) {
                Some(&(match_start, match_end)) if match_start <= position => (match_end.min(end), true),
                Some(&(match_start, _)) => (match_start.min(end), false),
                None => (end, false),
            };
            let piece_style = if matched {
                span.style.patch(MATCH_STYLE)
            } else {
                span.style
            };
            spans.push(Span::styled(
                content[position - start..piece_end - start].to_string(),
                piece_style,
            ));
            position = piece_end;
        }
        offset = end;
    }
    (
        Line {
            spans,
            style,
            alignment,
        },
        match_ends,
    )
}

/// Highlights all lines; returns them with every match as (line index, match end offset)
fn highlight_lines(
    lines: Vec<Line<'static>>,
    regex: &Regex,
) -> (Vec<Line<'static>>, Vec<(usize, usize)>) {
    let mut matches = Vec::new();
    let lines = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let (line, match_ends) = highlight_line(line, regex);
            matches.extend(match_ends.into_iter().map(|end| (index, end)));
            line
        })
        .collect();
    (lines, matches)
}

fn preview_lines(app: &App, tab: TabBarState) -> Vec<Line<'static>> {
    if tab == TabBarState::Help {
        return help_lines();
    }
    match app.selected_entry().and_then(|entry| app.har.log.entries.get(entry)) {
        Some(entry) => match tab {
            TabBarState::Headers => headers_lines(entry, app.body_options),
            TabBarState::Cookies => cookies_lines(entry),
            TabBarState::Request => request_lines(entry, app.body_options),
            TabBarState::Response => response_lines(entry, app.body_options),
            TabBarState::Help => help_lines(),
        },
        None => vec![placeholder("No entry selected")],
    }
}

fn tabbar(state: TabBarState) -> Tabs<'static> {
    Tabs::new(TabBarState::ALL.iter().map(|tab| tab.title()).collect::<Vec<_>>())
        .select(state.to_index())
        .padding(" ", " ")
}

fn help_lines() -> Vec<Line<'static>> {
    let width = KEY_HELP
        .iter()
        .flat_map(|(_, keys)| keys.iter().map(|(key, _)| key.len()))
        .max()
        .unwrap_or(0);

    let mut lines = Vec::new();
    for (title, keys) in KEY_HELP {
        lines.push(section(title));
        lines.extend(keys.iter().map(|(key, action)| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<width$}  ", key, width = width),
                    Style::default().fg(Color::LightCyan).bold(),
                ),
                Span::raw(*action),
            ])
        }));
        lines.push(Line::default());
    }
    lines
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(Color::LightYellow).bold(),
    ))
}

fn key_value(name: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{}: ", name),
            Style::default().fg(Color::LightCyan).bold(),
        ),
        Span::raw(value.to_string()),
    ])
}

fn placeholder(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(Color::DarkGray).italic(),
    ))
}

fn pairs_section(title: &str, pairs: Vec<(&str, &str)>) -> Vec<Line<'static>> {
    let mut lines = vec![section(&format!("{} ({})", title, pairs.len()))];
    if pairs.is_empty() {
        lines.push(placeholder("(none)"));
    }
    lines.extend(pairs.into_iter().map(|(name, value)| key_value(name, value)));
    lines.push(Line::default());
    lines
}

/// Like `pairs_section`, but values are decoded (URL, base64) when decoding is on
fn fields_section(title: &str, pairs: Vec<(&str, &str)>, options: BodyOptions) -> Vec<Line<'static>> {
    let mut lines = vec![section(&format!("{} ({})", title, pairs.len()))];
    if pairs.is_empty() {
        lines.push(placeholder("(none)"));
    }
    for (name, value) in pairs {
        let name = if options.decode {
            body::percent_decode(name, true)
        } else {
            name.into()
        };
        let field = body::decode_field(value, options);

        let mut label = vec![Span::styled(
            format!("{}: ", name),
            Style::default().fg(Color::LightCyan).bold(),
        )];
        if field.base64 {
            label.push(Span::styled("[base64] ", Style::default().fg(Color::LightGreen)));
        }
        // Multi-line values (e.g. a decoded, pretty printed SAML assertion) go below the name
        if field.text.contains('\n') {
            lines.push(Line::from(label));
            lines.extend(field.text.lines().map(|line| Line::from(format!("  {}", line))));
        } else {
            label.push(Span::raw(field.text.into_owned()));
            lines.push(Line::from(label));
        }
    }
    lines.push(Line::default());
    lines
}

fn header_pairs(headers: &[Header]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect()
}

fn opt_ms(value: Option<f64>) -> String {
    match value {
        Some(v) if v >= 0.0 => format!("{:.1} ms", v),
        _ => "-".to_string(),
    }
}

/// HAR uses -1 for unknown sizes
fn opt_bytes(value: Option<i64>) -> String {
    match value {
        Some(v) if v >= 0 => format!("{} bytes", v),
        _ => "-".to_string(),
    }
}

fn headers_lines(entry: &Entry, options: BodyOptions) -> Vec<Line<'static>> {
    let req = &entry.request;
    let resp = &entry.response;
    let t = &entry.timings;

    let mut lines = vec![
        section("General"),
        key_value("URL", req.url.as_str()),
        key_value("Method", &req.method),
        key_value(
            "Status",
            &format!("{} {}", resp.status, resp.status_text),
        ),
        key_value("HTTP Version", &req.http_version),
        key_value(
            "Remote Address",
            entry.server_ipaddress.as_deref().unwrap_or("-"),
        ),
        key_value("Started", &entry.started_date_time),
        key_value("Total Time", &opt_ms(Some(entry.time))),
        key_value(
            "Timings",
            &format!(
                "blocked {} | dns {} | connect {} | ssl {} | send {} | wait {} | receive {}",
                opt_ms(t.blocked),
                opt_ms(t.dns),
                opt_ms(t.connect),
                opt_ms(t.ssl),
                opt_ms(t.send),
                opt_ms(t.wait),
                opt_ms(t.receive)
            ),
        ),
    ];
    if !resp.redirect_url.is_empty() {
        lines.push(key_value("Redirect URL", &resp.redirect_url));
    }
    lines.push(Line::default());

    lines.extend(fields_section(
        "Query Parameters",
        req.query_string
            .iter()
            .map(|q| (q.name.as_str(), q.value.as_str()))
            .collect(),
        options,
    ));
    lines.extend(pairs_section("Response Headers", header_pairs(&resp.headers)));
    lines.extend(pairs_section("Request Headers", header_pairs(&req.headers)));
    lines
}

fn cookies_lines(entry: &Entry) -> Vec<Line<'static>> {
    let mut lines = pairs_section(
        "Request Cookies",
        entry
            .request
            .cookies
            .iter()
            .map(|c| (c.name.as_str(), c.value.as_str()))
            .collect(),
    );
    lines.extend(pairs_section(
        "Response Cookies",
        entry
            .response
            .cookies
            .iter()
            .map(|c| (c.name.as_str(), c.value.as_str()))
            .collect(),
    ));
    lines
}

fn request_lines(entry: &Entry, options: BodyOptions) -> Vec<Line<'static>> {
    let req = &entry.request;
    let mut lines = vec![
        section("Request"),
        key_value("Method", &req.method),
        key_value("Body Size", &opt_bytes(Some(req.body_size))),
        Line::default(),
    ];

    match &req.post_data {
        Some(post_data) => {
            lines.push(key_value("Content Type", &post_data.mime_type));
            lines.push(Line::default());
            // Not every exporter records `params`, so fall back to parsing a form body
            let params: Option<Vec<(&str, &str)>> = match &post_data.params {
                Some(params) if !params.is_empty() => Some(
                    params
                        .iter()
                        .map(|p| (p.name.as_str(), p.value.as_str()))
                        .collect(),
                ),
                _ if body::is_form_urlencoded(&post_data.mime_type) => {
                    Some(body::form_fields(&post_data.text))
                }
                _ => None,
            };
            if let Some(params) = params {
                lines.extend(fields_section("Form Parameters", params, options));
            }
            lines.push(section("Body"));
            let request_body = Body {
                text: Some(&post_data.text),
                encoding: None,
                mime_type: Some(&post_data.mime_type),
            };
            lines.extend(processed_lines(body::process(&request_body, options)));
        }
        None => lines.push(placeholder("(no request body)")),
    }
    lines
}

fn response_lines(entry: &Entry, options: BodyOptions) -> Vec<Line<'static>> {
    let resp = &entry.response;
    let content = &resp.content;
    let mut lines = vec![
        section("Response"),
        key_value(
            "Status",
            &format!("{} {}", resp.status, resp.status_text),
        ),
        key_value(
            "Content Type",
            content.mime_type.as_deref().unwrap_or("-"),
        ),
        key_value(
            "Size",
            &opt_bytes(content.size),
        ),
    ];
    if let Some(encoding) = &content.encoding {
        lines.push(key_value("Encoding", encoding));
    }
    lines.push(Line::default());
    lines.push(section("Body"));

    let response_body = Body {
        text: content.text.as_deref(),
        encoding: content.encoding.as_deref(),
        mime_type: content.mime_type.as_deref(),
    };
    lines.extend(processed_lines(body::process(&response_body, options)));
    lines
}

fn processed_lines(processed: Processed) -> Vec<Line<'static>> {
    match processed {
        Processed::Text(text) => text.lines().map(|line| Line::from(line.to_string())).collect(),
        Processed::Empty => vec![placeholder("(empty)")],
        Processed::NotCaptured => vec![placeholder("(body not captured in HAR)")],
        Processed::Encoded { encoding, len } => vec![placeholder(&format!(
            "({}-encoded body, {} chars - press x to decode)",
            encoding, len
        ))],
        Processed::InvalidEncoding { encoding, error } => vec![placeholder(&format!(
            "(could not decode {} body: {})",
            encoding, error
        ))],
        Processed::Binary(bytes) => binary_lines(&bytes),
    }
}

/// Larger binaries are truncated; use the pager or export to see everything
const HEX_DUMP_LIMIT: usize = 4096;

fn binary_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    let mut lines = vec![placeholder(&format!("(binary content, {} bytes)", bytes.len()))];
    lines.push(Line::default());
    lines.extend(
        body::hex_dump(bytes, HEX_DUMP_LIMIT)
            .into_iter()
            .map(|line| Line::styled(line, Style::default().fg(Color::Gray))),
    );
    if bytes.len() > HEX_DUMP_LIMIT {
        lines.push(placeholder(&format!(
            "... {} more bytes not shown",
            bytes.len() - HEX_DUMP_LIMIT
        )));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(line: &Line) -> Vec<(String, bool)> {
        line.spans
            .iter()
            .map(|span| (span.content.to_string(), span.style.bg == Some(Color::Yellow)))
            .collect()
    }

    #[test]
    fn highlight_line_splits_match_out_of_span() {
        let regex = Regex::new("b+").expect("regex");

        let (line, match_ends) = highlight_line(Line::from("abbbc"), &regex);

        assert_eq!(
            (match_ends, spans(&line)),
            (
                vec![4],
                vec![
                    ("a".to_string(), false),
                    ("bbb".to_string(), true),
                    ("c".to_string(), false)
                ]
            )
        );
    }

    #[test]
    fn highlight_line_match_across_spans_keeps_span_styles() {
        let regex = Regex::new("name: va").expect("regex");
        let line = Line::from(vec![
            Span::styled("name: ", Style::default().fg(Color::Cyan)),
            Span::raw("value"),
        ]);

        let (line, _) = highlight_line(line, &regex);

        assert_eq!(
            (spans(&line), line.spans[0].style.fg),
            (
                vec![
                    ("name: ".to_string(), true),
                    ("va".to_string(), true),
                    ("lue".to_string(), false)
                ],
                Some(Color::Black)
            )
        );
    }

    #[test]
    fn highlight_line_multiple_matches_and_multibyte_text() {
        let regex = Regex::new("é").expect("regex");

        let (line, _) = highlight_line(Line::from("café olé"), &regex);

        assert_eq!(
            spans(&line),
            vec![
                ("caf".to_string(), false),
                ("é".to_string(), true),
                (" ol".to_string(), false),
                ("é".to_string(), true)
            ]
        );
    }

    #[test]
    fn highlight_line_without_match_is_unchanged() {
        let regex = Regex::new("zzz").expect("regex");

        let (line, match_ends) = highlight_line(Line::from("abc"), &regex);

        assert_eq!(
            (match_ends.is_empty(), spans(&line)),
            (true, vec![("abc".to_string(), false)])
        );
    }

    #[test]
    fn highlight_lines_reports_every_match() {
        let regex = Regex::new("x").expect("regex");
        let lines = vec![Line::from("x"), Line::from("y"), Line::from("axbx")];

        let (_, matches) = highlight_lines(lines, &regex);

        assert_eq!(matches, vec![(0, 1), (2, 2), (2, 4)]);
    }

    #[test]
    fn match_row_finds_row_of_match_inside_wrapped_line() {
        let lines = vec![
            Line::from("intro"),                     // row 0
            Line::from("aaaa bbbb cccc dddd tgt"), // rows 1-5 at width 5, `tgt` on row 5
        ];

        assert_eq!(match_row(&lines, 1, 23, 5), 5);
    }

    #[test]
    fn wrapped_row_counts_wrapped_and_empty_lines() {
        let lines = vec![
            Line::from("aaaa bbbb cccc"), // 3 rows at width 5
            Line::default(),              // 1 row
            Line::from("target"),
        ];

        assert_eq!(wrapped_row(&lines, 2, 5), 4);
    }
}
