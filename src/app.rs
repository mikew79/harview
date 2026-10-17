use mime;
use ratatui::{prelude::*, widgets::*};

#[derive(Debug)]
pub struct App {
    pub running: bool,
    index: usize,
    pub har: Har,
    pub tabbar_state: TabBarState,
    pub preview_scroll: u16,
    /// Built once at startup; the HAR is immutable while viewing
    pub table_items: Vec<TableItem>,
    /// Kept across frames so the table's scroll offset persists
    pub table_state: TableState,
    /// Preview lines for the current (entry, tab), rebuilt only when either changes
    pub preview_cache: Option<PreviewCache>,
}

#[derive(Debug)]
pub struct PreviewCache {
    pub key: (usize, TabBarState),
    pub lines: Vec<Line<'static>>,
}

impl App {
    pub fn init(har: Har) -> Self {
        Self {
            running: true,
            index: 0,
            tabbar_state: TabBarState::Headers,
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

    pub fn set_tabbar_state(&mut self, state: &TabBarState) {
        self.tabbar_state = *state;
        self.preview_scroll = 0;
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
}

impl TabBarState {
    pub fn to_index(&self) -> usize {
        match self {
            Self::Headers => 0,
            Self::Cookies => 1,
            Self::Request => 2,
            Self::Response => 3,
        }
    }
}

impl ToString for TabBarState {
    fn to_string(&self) -> String {
        match self {
            Self::Headers => " [1] Headers ".to_string(),
            Self::Cookies => " [2] Cookies ".to_string(),
            Self::Request => " [3] Request ".to_string(),
            Self::Response => " [4] Response ".to_string(),
        }
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
