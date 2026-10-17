use crate::body::BodyOptions;
use crate::export::{self, ExportTarget};
use crate::search::{self, EntryText, Query};
use mime;
use ratatui::{prelude::*, widgets::*};
use regex::Regex;

/// Input mode; decides how key events are interpreted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Input(InputKind),
    /// The `e` export popup
    ExportMenu,
}

/// What the input bar is editing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// `/`: filters the entries table
    Filter,
    /// `f`: searches the current preview only
    Find,
    /// File name for an export
    ExportPath,
}

/// Choices made in the export popup
#[derive(Debug, Clone, PartialEq)]
pub struct ExportState {
    pub target: ExportTarget,
    /// Decoded / pretty printed as in the preview, or exactly as stored in the HAR
    pub decoded: bool,
    /// Path the user was warned already exists; submitting it again overwrites
    overwrite_path: Option<String>,
}

/// Single-line text input with a cursor (counted in chars)
#[derive(Debug, Default)]
pub struct Input {
    text: String,
    cursor: usize,
}

impl Input {
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = text.chars().count();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn byte_offset(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(offset, _)| offset)
    }

    pub fn insert(&mut self, c: char) {
        let offset = self.byte_offset();
        self.text.insert(offset, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            let offset = self.byte_offset();
            self.text.remove(offset);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.chars().count() {
            let offset = self.byte_offset();
            self.text.remove(offset);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text.chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn clear(&mut self) {
        self.set("");
    }
}

/// Status bar message, cleared on the next key press
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// A pending jump between highlighted matches, resolved when the preview is next rendered
/// (only then are the matches and the wrapped layout known)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchJump {
    /// First match; with `any_tab`, switch to the first tab that has one if this tab has none
    First { any_tab: bool },
    Next,
    Prev,
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    /// Position of the selection within `visible` (not an entry index)
    index: usize,
    pub har: Har,
    /// Indexes of the entries shown in the table, in display order (all of them unless filtered)
    visible: Vec<usize>,
    /// Name of the loaded file, shown in the status bar
    pub source: String,
    pub mode: Mode,
    pub tabbar_state: TabBarState,
    pub body_options: BodyOptions,
    pub preview_scroll: u16,
    /// `z`: the preview fills the screen and the table is hidden
    pub preview_maximized: bool,
    pub input: Input,
    /// Active `/` filter: its text and parsed query
    pub filter: Option<(String, Query)>,
    /// Active `f` find: its text and regex
    pub find: Option<(String, Regex)>,
    /// Decoded search text per entry, built on the first filter
    search_texts: Option<Vec<EntryText>>,
    /// Bumped whenever the filter or find changes, so the preview is re-highlighted
    highlight_generation: u64,
    pub message: Option<Message>,
    pub export: ExportState,
    /// OSC 52 sequence waiting to be written to the terminal
    pending_clipboard: Option<String>,
    pub match_jump: Option<MatchJump>,
    /// Index into the preview's matches of the one last jumped to
    pub match_cursor: Option<usize>,
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
    /// Entry index, `None` when nothing is selected (e.g. a filter matched nothing)
    pub entry: Option<usize>,
    pub tab: TabBarState,
    pub body_options: BodyOptions,
    pub highlight_generation: u64,
}

#[derive(Debug)]
pub struct PreviewCache {
    pub key: PreviewKey,
    pub lines: Vec<Line<'static>>,
    /// Every highlighted match, in order: (line index, byte offset of the match end in the line)
    pub matches: Vec<(usize, usize)>,
    /// When this tab has no matches: the first other tab that does
    pub match_elsewhere: Option<TabBarState>,
    /// Wrapped layout for the last width rendered at
    pub layout: Option<WrappedLayout>,
}

/// Row on which each preview line starts once wrapped at `width`, plus a final entry holding
/// the total, so only the lines on screen need rendering
#[derive(Debug)]
pub struct WrappedLayout {
    pub width: u16,
    pub starts: Vec<usize>,
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
            preview_maximized: false,
            input: Input::default(),
            filter: None,
            find: None,
            search_texts: None,
            highlight_generation: 0,
            message: None,
            export: ExportState {
                target: ExportTarget::ResponseBody,
                decoded: true,
                overwrite_path: None,
            },
            pending_clipboard: None,
            match_jump: None,
            match_cursor: None,
            table_items: har.to_table_items(),
            visible: (0..har.log.entries.len()).collect(),
            table_state: TableState::default().with_selected(Some(0)),
            preview_cache: None,
            har: har,
        }
    }

    pub fn tick(&self) {}

    /// Position of the selection within the visible entries
    pub fn get_index(&self) -> usize {
        self.index
    }

    /// Number of visible entries
    pub fn max_index(&self) -> usize {
        self.visible.len()
    }

    pub fn total_entries(&self) -> usize {
        self.har.log.entries.len()
    }

    pub fn visible(&self) -> &[usize] {
        &self.visible
    }

    /// Index into `har.log.entries` of the selected entry
    pub fn selected_entry(&self) -> Option<usize> {
        self.visible.get(self.index).copied()
    }

    /// Replaces the visible entries, keeping the selected entry selected when it's still visible
    pub fn set_visible(&mut self, visible: Vec<usize>) {
        let selected = self.selected_entry();
        self.visible = visible;
        let index = selected
            .and_then(|entry| self.visible.iter().position(|&visible| visible == entry))
            .unwrap_or(0);
        self.set_position(index);
        if self.selected_entry() != selected {
            self.preview_scroll = 0;
        }
    }

    pub fn show_all_entries(&mut self) {
        self.set_visible((0..self.total_entries()).collect());
    }

    fn select(&mut self, index: usize) {
        if self.selected_entry() != self.visible.get(index).copied() {
            self.preview_scroll = 0;
        }
        self.set_position(index);
    }

    fn set_position(&mut self, index: usize) {
        self.index = index;
        self.table_state
            .select((!self.visible.is_empty()).then_some(index));
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
            entry: self.selected_entry(),
            tab: self.tabbar_state,
            body_options: self.body_options,
            highlight_generation: self.highlight_generation,
        }
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    /// Opens the input bar, prefilled with the active filter / find
    pub fn start_input(&mut self, kind: InputKind) {
        let current = match kind {
            InputKind::Filter => self.filter.as_ref().map(|(text, _)| text.as_str()),
            InputKind::Find => self.find.as_ref().map(|(text, _)| text.as_str()),
            InputKind::ExportPath => return self.export_to_file(),
        };
        self.input.set(current.unwrap_or(""));
        self.mode = Mode::Input(kind);
    }

    /// Closes the input bar; cancelling the export file name goes back to the export popup
    pub fn cancel_input(&mut self) {
        self.mode = match self.mode {
            Mode::Input(InputKind::ExportPath) => Mode::ExportMenu,
            _ => Mode::Normal,
        };
    }

    /// Applies the input; on error the input stays open so it can be corrected
    pub fn submit_input(&mut self) {
        let Mode::Input(kind) = self.mode else {
            return;
        };
        let text = self.input.text().to_string();
        let result = match kind {
            InputKind::Filter => self.apply_filter(&text),
            InputKind::Find => self.apply_find(&text),
            InputKind::ExportPath => self.save_export(&text),
        };
        match result {
            Ok(()) => self.mode = Mode::Normal,
            Err(error) => self.error(error),
        }
    }

    /// Opens the export popup, defaulting to what the current tab shows
    pub fn open_export(&mut self) {
        if self.selected_entry().is_none() {
            self.error("no entry selected");
            return;
        }
        self.export.target = match self.tabbar_state {
            TabBarState::Request => ExportTarget::RequestBody,
            TabBarState::Response => ExportTarget::ResponseBody,
            _ => ExportTarget::FullRequest,
        };
        self.export.overwrite_path = None;
        self.mode = Mode::ExportMenu;
    }

    pub fn close_export(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn select_export_target(&mut self, target: ExportTarget) {
        self.export.target = target;
    }

    pub fn cycle_export_target(&mut self, delta: i32) {
        let all = ExportTarget::ALL;
        let index = all.iter().position(|&target| target == self.export.target).unwrap_or(0);
        let next = (index as i32 + delta).rem_euclid(all.len() as i32) as usize;
        self.export.target = all[next];
    }

    pub fn toggle_export_decoded(&mut self) {
        self.export.decoded = !self.export.decoded;
    }

    fn build_export(&self) -> Result<export::Exported, String> {
        let entry = self
            .selected_entry()
            .and_then(|index| self.har.log.entries.get(index))
            .ok_or("no entry selected")?;
        export::build(
            entry,
            self.export.target,
            self.export.decoded,
            self.body_options.pretty,
        )
    }

    /// Asks for a file name, prefilled with a default under `exports/`
    pub fn export_to_file(&mut self) {
        let Some(index) = self.selected_entry() else {
            self.error("no entry selected");
            return;
        };
        let path = export::default_path(
            index + 1,
            &self.har.log.entries[index],
            self.export.target,
            self.export.decoded,
        );
        self.input.set(&path);
        self.export.overwrite_path = None;
        self.mode = Mode::Input(InputKind::ExportPath);
    }

    /// Writes the export; an existing file is only replaced when the same path is submitted twice
    pub fn save_export(&mut self, path: &str) -> Result<(), String> {
        let path = path.trim();
        if path.is_empty() {
            return Err("enter a file name".to_string());
        }
        let exported = self.build_export()?;
        let overwrite = self.export.overwrite_path.as_deref() == Some(path);
        match export::write_file(std::path::Path::new(path), &exported.bytes, overwrite) {
            Ok(()) => {
                self.export.overwrite_path = None;
                self.info(format!("saved {} ({} bytes)", path, exported.bytes.len()));
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.export.overwrite_path = Some(path.to_string());
                Err("file exists: Enter again to overwrite".to_string())
            }
            Err(error) => Err(format!("could not save {}: {}", path, error)),
        }
    }

    /// Queues the export for the terminal's clipboard (OSC 52)
    pub fn export_to_clipboard(&mut self) {
        let result = self.build_export().and_then(|exported| {
            export::osc52(&exported).map(|sequence| (sequence, exported.bytes.len()))
        });
        match result {
            Ok((sequence, len)) => {
                self.pending_clipboard = Some(sequence);
                self.mode = Mode::Normal;
                self.info(format!("copied {} bytes to the clipboard", len));
            }
            Err(error) => self.error(error),
        }
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard.take()
    }

    /// Filters the table; an empty query shows everything again
    pub fn apply_filter(&mut self, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            self.clear_filter();
            return Ok(());
        }
        let query = Query::parse(text)?;

        if self.search_texts.is_none() {
            self.search_texts = Some(self.har.log.entries.iter().map(EntryText::new).collect());
        }
        let texts = self.search_texts.as_deref().unwrap_or_default();
        let visible: Vec<usize> = self
            .har
            .log
            .entries
            .iter()
            .zip(texts)
            .enumerate()
            .filter(|(_, (entry, text))| query.matches(entry, text))
            .map(|(index, _)| index)
            .collect();

        if visible.is_empty() {
            self.info("no entries match");
        }
        self.set_visible(visible);
        self.filter = Some((text.to_string(), query));
        self.highlight_changed(MatchJump::First { any_tab: true });
        Ok(())
    }

    pub fn clear_filter(&mut self) {
        self.filter = None;
        self.show_all_entries();
        self.highlight_generation += 1;
    }

    /// Highlights matches in the current preview; an empty pattern clears the find
    pub fn apply_find(&mut self, text: &str) -> Result<(), String> {
        if text.is_empty() {
            self.clear_find();
            return Ok(());
        }
        self.find = Some((text.to_string(), search::smart_regex(text)?));
        self.highlight_changed(MatchJump::First { any_tab: false });
        Ok(())
    }

    pub fn clear_find(&mut self) {
        self.find = None;
        self.highlight_generation += 1;
    }

    /// `Esc`: clears the find first, then the filter
    pub fn clear_search(&mut self) {
        if self.find.is_some() {
            self.clear_find();
        } else if self.filter.is_some() {
            self.clear_filter();
        }
    }

    fn highlight_changed(&mut self, jump: MatchJump) {
        self.highlight_generation += 1;
        self.match_jump = Some(jump);
    }

    pub fn jump_to_match(&mut self, jump: MatchJump) {
        if self.highlight().is_some() {
            self.match_jump = Some(jump);
        } else {
            self.info("no active search (/ to filter, f to find)");
        }
    }

    /// Regex highlighted in the preview: the find if active, otherwise the filter's
    pub fn highlight(&self) -> Option<&Regex> {
        self.find
            .as_ref()
            .map(|(_, regex)| regex)
            .or_else(|| self.table_highlight())
    }

    /// Regex highlighted in the table: the filter's (find is preview-only)
    pub fn table_highlight(&self) -> Option<&Regex> {
        self.filter.as_ref().and_then(|(_, query)| query.highlight())
    }

    pub fn set_tabbar_state(&mut self, state: &TabBarState) {
        self.tabbar_state = *state;
        self.preview_scroll = 0;
    }

    pub fn toggle_pretty(&mut self) {
        self.body_options.pretty = !self.body_options.pretty;
        self.preview_scroll = 0;
        self.rejump_after_content_change();
    }

    pub fn toggle_preview_maximized(&mut self) {
        self.preview_maximized = !self.preview_maximized;
    }

    pub fn toggle_highlight(&mut self) {
        self.body_options.highlight = !self.body_options.highlight;
    }

    pub fn toggle_decode(&mut self) {
        self.body_options.decode = !self.body_options.decode;
        self.preview_scroll = 0;
        self.rejump_after_content_change();
    }

    /// Decoding or pretty printing changes what the preview shows (e.g. reveals a match inside
    /// a base64 field), so jump to the first match again
    fn rejump_after_content_change(&mut self) {
        if self.highlight().is_some() {
            self.match_jump = Some(MatchJump::First {
                any_tab: self.find.is_none(),
            });
        }
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
    pub fn to_table_row(&self, highlight: Option<&Regex>) -> ratatui::widgets::Row<'static> {
        let highlighted = |text: &str, style: Style| {
            let line = Line::styled(text.to_string(), style);
            match highlight {
                Some(regex) => crate::ui::highlight_line(line, regex).0,
                None => line,
            }
        };
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
            Cell::new(highlighted(&self.domain, Style::default().fg(Color::White))),
            Cell::new(highlighted(&self.file_name, Style::default())),
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
    fn set_visible_keeps_selected_entry_when_still_visible() {
        let mut app = app_with_entries(5);
        app.update_index(3);

        app.set_visible(vec![1, 3, 4]);

        assert_eq!((app.get_index(), app.selected_entry()), (1, Some(3)));
    }

    #[test]
    fn set_visible_without_selected_entry_selects_first() {
        let mut app = app_with_entries(5);
        app.update_index(2);

        app.set_visible(vec![0, 4]);

        assert_eq!(app.selected_entry(), Some(0));
    }

    #[test]
    fn set_visible_empty_selects_nothing() {
        let mut app = app_with_entries(3);

        app.set_visible(vec![]);

        assert_eq!(
            (app.selected_entry(), app.max_index(), app.table_state.selected()),
            (None, 0, None)
        );
    }

    #[test]
    fn update_index_moves_within_visible_entries() {
        let mut app = app_with_entries(6);
        app.set_visible(vec![1, 3, 5]);

        app.update_index(1);

        assert_eq!(app.selected_entry(), Some(3));
    }

    #[test]
    fn show_all_entries_after_filter_keeps_selection() {
        let mut app = app_with_entries(6);
        app.set_visible(vec![2, 5]);
        app.update_index(1);

        app.show_all_entries();

        assert_eq!((app.get_index(), app.selected_entry()), (5, Some(5)));
    }

    #[test]
    fn set_visible_keeping_selection_keeps_scroll() {
        let mut app = app_with_entries(3);
        app.scroll_preview(4);

        app.set_visible(vec![0, 2]);

        assert_eq!(app.preview_scroll, 4);
    }

    #[test]
    fn set_visible_moving_selected_entry_keeps_scroll() {
        let mut app = app_with_entries(5);
        app.update_index(3);
        app.scroll_preview(4);

        app.set_visible(vec![1, 3]);

        assert_eq!((app.get_index(), app.preview_scroll), (1, 4));
    }

    fn app_from_fixture() -> App {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.har");
        let har = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture"))
            .expect("valid HAR");
        App::init(har, "sample.har".to_string())
    }

    #[test]
    fn input_insert_and_edit_at_cursor() {
        let mut input = Input::default();
        input.set("ac");
        input.left();
        input.insert('b');
        input.end();
        input.backspace();
        input.home();
        input.delete();

        assert_eq!((input.text(), input.cursor()), ("b", 0));
    }

    #[test]
    fn input_handles_multibyte_characters() {
        let mut input = Input::default();
        input.set("café");
        input.backspace();
        input.insert('e');

        assert_eq!(input.text(), "cafe");
    }

    #[test]
    fn apply_filter_shows_only_matching_entries() {
        let mut app = app_from_fixture();

        app.apply_filter("status:4xx").expect("valid query");

        // the POST /login fixture is the only 4xx
        assert_eq!(app.visible(), &[1]);
    }

    #[test]
    fn apply_filter_matches_decoded_saml_response() {
        let mut app = app_from_fixture();

        app.apply_filter("NameID>alice").expect("valid query");

        // the SAML form post, last fixture entry
        assert_eq!(app.visible(), &[10]);
    }

    #[test]
    fn apply_filter_invalid_query_keeps_previous_view() {
        let mut app = app_from_fixture();

        let result = app.apply_filter("body:(");

        assert_eq!(
            (result.is_err(), app.max_index(), app.filter.is_none()),
            (true, 11, true)
        );
    }

    #[test]
    fn apply_filter_empty_query_clears_filter() {
        let mut app = app_from_fixture();
        app.apply_filter("status:4xx").expect("valid query");

        app.apply_filter("  ").expect("empty query");

        assert_eq!((app.max_index(), app.filter.is_none()), (11, true));
    }

    #[test]
    fn submit_input_with_error_stays_in_input_mode() {
        let mut app = app_from_fixture();
        app.start_input(InputKind::Filter);
        app.input.set("status:");

        app.submit_input();

        assert_eq!(
            (app.mode, app.message.as_ref().map(|m| m.error)),
            (Mode::Input(InputKind::Filter), Some(true))
        );
    }

    #[test]
    fn start_input_prefills_active_filter() {
        let mut app = app_from_fixture();
        app.apply_filter("method:POST").expect("valid query");

        app.start_input(InputKind::Filter);

        assert_eq!(app.input.text(), "method:POST");
    }

    #[test]
    fn clear_search_clears_find_before_filter() {
        let mut app = app_from_fixture();
        app.apply_filter("method:POST").expect("valid query");
        app.apply_find("username").expect("valid regex");

        app.clear_search();

        assert_eq!((app.find.is_none(), app.filter.is_some()), (true, true));
    }

    #[test]
    fn apply_find_does_not_filter_table() {
        let mut app = app_from_fixture();

        app.apply_find("alice").expect("valid regex");

        assert_eq!(app.max_index(), 11);
    }

    #[test]
    fn highlight_prefers_find_over_filter() {
        let mut app = app_from_fixture();
        app.apply_filter("login").expect("valid query");
        app.apply_find("password").expect("valid regex");

        let highlight = app.highlight().expect("highlight");

        assert_eq!(
            (highlight.is_match("password"), highlight.is_match("login")),
            (true, false)
        );
    }

    #[test]
    fn toggle_decode_with_active_filter_jumps_to_first_match_on_any_tab() {
        let mut app = app_from_fixture();
        app.apply_filter("NameID").expect("valid query");
        app.match_jump = None;

        app.toggle_decode();

        assert_eq!(app.match_jump, Some(MatchJump::First { any_tab: true }));
    }

    #[test]
    fn toggle_decode_without_search_does_not_jump() {
        let mut app = app_from_fixture();

        app.toggle_decode();

        assert_eq!(app.match_jump, None);
    }

    #[test]
    fn preview_key_after_filter_change_differs() {
        let mut app = app_from_fixture();
        let before = app.preview_key();

        app.apply_find("alice").expect("valid regex");

        assert_ne!(app.preview_key(), before);
    }

    #[test]
    fn open_export_defaults_target_to_current_tab() {
        let mut app = app_from_fixture();
        app.set_tabbar_state(&TabBarState::Request);

        app.open_export();

        assert_eq!(
            (app.mode, app.export.target),
            (Mode::ExportMenu, ExportTarget::RequestBody)
        );
    }

    #[test]
    fn cycle_export_target_wraps_both_ways() {
        let mut app = app_from_fixture();
        app.select_export_target(ExportTarget::RequestBody);

        app.cycle_export_target(-1);
        let wrapped_back = app.export.target;
        app.cycle_export_target(1);

        assert_eq!(
            (wrapped_back, app.export.target),
            (ExportTarget::FullResponse, ExportTarget::RequestBody)
        );
    }

    #[test]
    fn export_to_file_prefills_default_path() {
        let mut app = app_from_fixture();
        app.open_export();
        app.select_export_target(ExportTarget::ResponseBody);

        app.export_to_file();

        assert_eq!(
            (app.mode, app.input.text()),
            (
                Mode::Input(InputKind::ExportPath),
                "exports/0001-api.example.com-users-response-body.json"
            )
        );
    }

    #[test]
    fn cancel_input_from_export_path_returns_to_export_menu() {
        let mut app = app_from_fixture();
        app.open_export();
        app.export_to_file();

        app.cancel_input();

        assert_eq!(app.mode, Mode::ExportMenu);
    }

    #[test]
    fn save_export_existing_file_needs_second_submit() {
        let dir = std::env::temp_dir().join(format!("harview-app-test-{}", std::process::id()));
        let path = dir.join("out.json").to_string_lossy().into_owned();
        let mut app = app_from_fixture();
        app.select_export_target(ExportTarget::ResponseBody);

        let first = app.save_export(&path);
        let second = app.save_export(&path);
        let third = app.save_export(&path);
        std::fs::remove_dir_all(&dir).expect("cleanup");

        assert_eq!(
            (first.is_ok(), second.is_err(), third.is_ok()),
            (true, true, true)
        );
    }

    #[test]
    fn export_to_clipboard_queues_osc52_sequence() {
        let mut app = app_from_fixture();
        app.open_export();
        app.select_export_target(ExportTarget::ResponseBody);

        app.export_to_clipboard();

        let sequence = app.take_clipboard().expect("queued");
        assert_eq!(
            (sequence.starts_with("\x1b]52;c;"), app.mode, app.take_clipboard()),
            (true, Mode::Normal, None)
        );
    }

    #[test]
    fn export_to_clipboard_of_binary_stays_in_menu_with_error() {
        let mut app = app_from_fixture();
        app.update_index(8); // the base64 PNG fixture
        app.open_export();
        app.select_export_target(ExportTarget::ResponseBody);

        app.export_to_clipboard();

        assert_eq!(
            (app.mode, app.message.as_ref().map(|m| m.error), app.take_clipboard()),
            (Mode::ExportMenu, Some(true), None)
        );
    }

    #[test]
    fn toggle_preview_maximized_twice_restores_split() {
        let mut app = app_with_entries(1);

        app.toggle_preview_maximized();
        let maximized = app.preview_maximized;
        app.toggle_preview_maximized();

        assert_eq!((maximized, app.preview_maximized), (true, false));
    }

    #[test]
    fn toggle_preview_maximized_keeps_selection_and_scroll() {
        let mut app = app_with_entries(3);
        app.update_index(2);
        app.scroll_preview(4);

        app.toggle_preview_maximized();

        assert_eq!((app.selected_entry(), app.preview_scroll), (Some(2), 4));
    }

    #[test]
    fn preview_key_after_tab_change_differs() {
        let mut app = app_with_entries(1);
        let before = app.preview_key();

        app.next_tab();

        assert_ne!(app.preview_key(), before);
    }
}
