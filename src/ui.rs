use crate::app::{App, Mode, PreviewCache, TabBarState};
use crate::body::{self, Body, BodyOptions, Processed};
use crate::handler::KEY_HELP;
use crate::har::{Entry, Header};
use ratatui::{prelude::*, widgets::*};

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
    render_status_bar(app, main_layout[2], frame.buffer_mut());
}

pub fn render_status_bar(app: &App, area: Rect, buf: &mut Buffer) {
    let mode = match app.mode {
        Mode::Normal => " NORMAL ",
    };
    let total = app.max_index();
    let position = if total == 0 { 0 } else { app.get_index() + 1 };

    let left = Line::from(vec![
        Span::styled(mode, Style::default().fg(Color::Black).bg(Color::LightCyan).bold()),
        Span::raw(" "),
        Span::raw(app.source.clone()),
    ]);
    let flag = |label: &'static str, on: bool| {
        let style = if on {
            Style::default().fg(Color::LightGreen)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Span::styled(label, style)
    };
    let right = Line::from(vec![
        flag("pretty ", app.body_options.pretty),
        flag("decode ", app.body_options.decode),
        Span::raw(format!(" {}/{} ", position, total)),
        Span::styled(" ?: help  q: quit ", Style::default().fg(Color::DarkGray)),
    ])
    .right_aligned();

    Widget::render(left, area, buf);
    Widget::render(right, area, buf);
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

    let rows = app.table_items.iter().map(|item| item.to_table_row());

    let table = Table::new(rows, widths)
        .header(headers)
        .highlight_style(Style::default().reversed())
        .block(
            Block::default()
                .padding(Padding::horizontal(1))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    StatefulWidget::render(table, area, buf, &mut app.table_state);
}

pub fn render_preview(app: &mut App, area: Rect, buf: &mut Buffer) {
    let key = app.preview_key();
    if app.preview_cache.as_ref().map(|cache| cache.key) != Some(key) {
        app.preview_cache = Some(PreviewCache {
            key,
            lines: preview_lines(app),
        });
    }
    let lines = app
        .preview_cache
        .as_ref()
        .map(|cache| cache.lines.clone())
        .unwrap_or_default();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .split(area);

    Widget::render(tabbar(app.tabbar_state), layout[0], buf);

    let block = Block::default()
        .padding(Padding::horizontal(1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(layout[1]);

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

fn preview_lines(app: &App) -> Vec<Line<'static>> {
    if app.tabbar_state == TabBarState::Help {
        return help_lines();
    }
    match app.har.log.entries.get(app.get_index()) {
        Some(entry) => match app.tabbar_state {
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
