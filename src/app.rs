use crate::body::BodyOptions;
use mime;
use ratatui::{prelude::*, widgets::*};

/// Input mode; decides how key events are interpreted.
/// Later phases add modes for search input, the export menu and the export path prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    index: usize,
    pub har: Har,
    /// Name of the loaded file, shown in the status bar
    pub source: String,
    pub mode: Mode,
    pub tabbar_state: TabBarState,
    pub body_options: BodyOptions,
    pub preview_scroll: u16,
    /// Built once at startup; the HAR is immutable while viewing
    pub table_items: Vec<TableItem>,
    /// Kept across frames so the table's scroll offset persists
    pub table_state: TableState,
    /// Preview lines for the current key, rebuilt only when the key changes
    pub preview_cache: Option<PreviewCache>,
}

/// Everything the preview content depends on
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewKey {
    pub index: usize,
    pub tab: TabBarState,
    pub body_options: BodyOptions,
}

#[derive(Debug)]
pub struct PreviewCache {
    pub key: PreviewKey,
    pub lines: Vec<Line<'static>>,
}

impl App {
    pub fn init(har: Har, source: String) -> Self {
        Self {
            running: true,
            index: 0,
            source,
            mode: Mode::Normal,
            tabbar_state: TabBarState::Headers,
            body_options: BodyOptions::default(),
            preview_scroll: 0,
            table_items: har.to_table_items(),
            table_state: TableState::default().with_selected(Some(0)),
            preview_cache: None,
            har: har,
        }
    }

    pub fn tick(&self) {}

    pub fn get_index(&self) -> usize {
        self.index
    }

    pub fn max_index(&self) -> usize {
        self.har.log.entries.len()
    }

    fn select(&mut self, index: usize) {
        self.index = index;
        self.table_state.select(Some(index));
        self.preview_scroll = 0;
    }

    pub fn update_index(&mut self, delta: i32) {
        let max = self.max_index();
        let added = self.index as i32 + delta;
        let index = if added < 0 {
            0
        } else if added >= max as i32 {
            max.saturating_sub(1)
        } else {
            added as usize
        };
        self.select(index);
    }

    pub fn update_index_first(&mut self) {
        self.select(0);
    }

    pub fn update_index_last(&mut self) {
        self.select(self.max_index().saturating_sub(1));
    }

    pub fn scroll_preview(&mut self, delta: i32) {
        self.preview_scroll = (self.preview_scroll as i32 + delta).clamp(0, u16::MAX as i32) as u16;
    }

    pub fn preview_key(&self) -> PreviewKey {
        PreviewKey {
            index: self.index,
            tab: self.tabbar_state,
            body_options: self.body_options,
        }
    }

    pub fn set_tabbar_state(&mut self, state: &TabBarState) {
        self.tabbar_state = *state;
        self.preview_scroll = 0;
    }

    pub fn toggle_pretty(&mut self) {
        self.body_options.pretty = !self.body_options.pretty;
        self.preview_scroll = 0;
    }

    pub fn toggle_decode(&mut self) {
        self.body_options.decode = !self.body_options.decode;
        self.preview_scroll = 0;
    }

    pub fn next_tab(&mut self) {
        self.set_tabbar_state(&self.tabbar_state.next());
    }

    pub fn prev_tab(&mut self) {
        self.set_tabbar_state(&self.tabbar_state.prev());
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}

type Har = crate::Har;

impl Har {
    pub fn to_table_items(&self) -> Vec<TableItem> {
        self.log
            .entries
            .iter()
            .map(|entry| {
                let url = &entry.request.url;

                TableItem {
                    status: entry.response.status as u16,
                    method: entry.request.method.clone(),
                    domain: url.domain().unwrap_or("").to_string(),
                    file_name: url.path().to_string(),
                    mime_type: entry
                        .response
                        .content
                        .mime_type
                        .clone()
                        .unwrap_or("".to_string()),
                    size: entry.response.content.size,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarState {
    Headers,
    Cookies,
    Request,
    Response,
    Help,
}

impl TabBarState {
    /// Tab bar order
    pub const ALL: [TabBarState; 5] = [
        Self::Headers,
        Self::Cookies,
        Self::Request,
        Self::Response,
        Self::Help,
    ];

    pub fn to_index(&self) -> usize {
        match self {
            Self::Headers => 0,
            Self::Cookies => 1,
            Self::Request => 2,
            Self::Response => 3,
            Self::Help => 4,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Headers => " [1] Headers ",
            Self::Cookies => " [2] Cookies ",
            Self::Request => " [3] Request ",
            Self::Response => " [4] Response ",
            Self::Help => " [5/?] Help ",
        }
    }

    pub fn next(&self) -> Self {
        Self::ALL[(self.to_index() + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        Self::ALL[(self.to_index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct TableItem {
    status: u16,
    method: String,
    domain: String,
    file_name: String,
    mime_type: String,
    size: Option<i64>,
}

impl TableItem {
    pub fn to_table_row(&self) -> ratatui::widgets::Row<'static> {
        let status_span = match self.status {
            100..=199 => Span::styled(
                self.status.to_string(),
                Style::default().fg(Color::LightBlue).bold(),
            ),
            200..=299 => Span::styled(
                self.status.to_string(),
                Style::default().fg(Color::LightGreen).bold(),
            ),
            300..=399 => Span::styled(
                self.status.to_string(),
                Style::default().fg(Color::LightCyan).bold(),
            ),
            400..=499 => Span::styled(
                self.status.to_string(),
                Style::default().fg(Color::LightYellow).bold(),
            ),
            500..=599 => Span::styled(
                self.status.to_string(),
                Style::default().fg(Color::LightMagenta).bold(),
            ),
            0 => Span::styled("---", Style::default().fg(Color::DarkGray).bold()),
            _ => Span::styled(
                self.status.to_string(),
                Style::default().bg(Color::DarkGray),
            ),
        };

        let mime_type = self.mime_type.clone();
        let shorten_mime = match mime_type.as_str().parse::<mime::Mime>() {
            Ok(m) => m.subtype().to_string(),
            Err(_) => mime_type,
        };

        let size_span = match self.size {
            // HAR uses -1 for unknown sizes
            Some(s) if s >= 0 => {
                let b = byte_unit::Byte::from_u64(s as u64);
                Span::styled(
                    format!(
                        "{:>8.2} {:<2}",
                        b.get_appropriate_unit(byte_unit::UnitType::Decimal)
                            .get_value(),
                        b.get_appropriate_unit(byte_unit::UnitType::Decimal)
                            .get_unit()
                    ),
                    Style::default(),
                )
            }
            _ => Span::styled("     --- B", Style::default().fg(Color::DarkGray)),
        };

        Row::new([
            Cell::new(status_span),
            Cell::new(Span::styled(
                self.method.clone(),
                Style::default().fg(Color::White).bold(),
            )),
            Cell::new(Span::styled(
                self.domain.clone(),
                Style::default().fg(Color::White),
            )),
            Cell::new(self.file_name.clone()),
            Cell::new(shorten_mime),
            Cell::new(size_span),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_entries(count: usize) -> App {
        let entry = r#"{
            "startedDateTime": "2026-10-17T10:00:00Z",
            "request": {"method": "GET", "url": "https://example.com/"},
            "response": {"status": 200, "content": {}}
        }"#;
        let entries = vec![entry; count].join(",");
        let har = serde_json::from_str(&format!(r#"{{"log": {{"entries": [{}]}}}}"#, entries))
            .expect("valid HAR");
        App::init(har, "test.har".to_string())
    }

    #[test]
    fn next_tab_on_last_tab_wraps_to_first() {
        assert_eq!(TabBarState::Help.next(), TabBarState::Headers);
    }

    #[test]
    fn prev_tab_on_first_tab_wraps_to_last() {
        assert_eq!(TabBarState::Headers.prev(), TabBarState::Help);
    }

    #[test]
    fn to_index_matches_position_in_all() {
        let indexes: Vec<usize> = TabBarState::ALL.iter().map(|tab| tab.to_index()).collect();

        assert_eq!(indexes, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn next_tab_with_scrolled_preview_resets_scroll() {
        let mut app = app_with_entries(1);
        app.scroll_preview(5);

        app.next_tab();

        assert_eq!((app.tabbar_state, app.preview_scroll), (TabBarState::Cookies, 0));
    }

    #[test]
    fn update_index_past_end_clamps_to_last_entry() {
        let mut app = app_with_entries(3);

        app.update_index(10);

        assert_eq!(app.get_index(), 2);
    }

    #[test]
    fn update_index_before_start_clamps_to_first_entry() {
        let mut app = app_with_entries(3);

        app.update_index(-10);

        assert_eq!(app.get_index(), 0);
    }

    #[test]
    fn update_index_last_with_no_entries_stays_at_zero() {
        let mut app = app_with_entries(0);

        app.update_index_last();

        assert_eq!(app.get_index(), 0);
    }

    #[test]
    fn toggle_decode_flips_option_and_resets_scroll() {
        let mut app = app_with_entries(1);
        app.scroll_preview(5);

        app.toggle_decode();

        assert_eq!((app.body_options.decode, app.preview_scroll), (true, 0));
    }

    #[test]
    fn toggle_pretty_twice_restores_default() {
        let mut app = app_with_entries(1);

        app.toggle_pretty();
        app.toggle_pretty();

        assert_eq!(app.body_options, BodyOptions::default());
    }

    #[test]
    fn preview_key_after_toggle_decode_differs() {
        let mut app = app_with_entries(1);
        let before = app.preview_key();

        app.toggle_decode();

        assert_ne!(app.preview_key(), before);
    }

    #[test]
    fn preview_key_after_tab_change_differs() {
        let mut app = app_with_entries(1);
        let before = app.preview_key();

        app.next_tab();

        assert_ne!(app.preview_key(), before);
    }
}
