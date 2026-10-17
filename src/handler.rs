use crate::app::{self, InputKind, MatchJump, Mode};
use crate::export::ExportTarget;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, PartialEq)]
pub enum Command {
    Quit,
    TableFocusDelta(i32),
    TableFocusTop,
    TableFocusBottom,
    PreviewScroll(i32),
    SetTabBarState(app::TabBarState),
    NextTab,
    PrevTab,
    TogglePretty,
    ToggleDecode,
    ToggleHighlight,
    StartInput(InputKind),
    NextMatch,
    PrevMatch,
    ClearSearch,
    InputChar(char),
    InputBackspace,
    InputDelete,
    InputLeft,
    InputRight,
    InputHome,
    InputEnd,
    InputClear,
    InputSubmit,
    InputCancel,
    OpenExport,
    ExportSelect(ExportTarget),
    ExportCycle(i32),
    ExportToggleDecoded,
    ExportToFile,
    ExportToClipboard,
    ExportCancel,
}

impl Command {
    pub fn exec(&self, app: &mut app::App) {
        // Messages are only shown until the next key press
        app.message = None;
        match self {
            Self::Quit => app.quit(),
            Self::TableFocusTop => app.update_index_first(),
            Self::TableFocusBottom => app.update_index_last(),
            Self::TableFocusDelta(count) => app.update_index(*count),
            Self::PreviewScroll(delta) => app.scroll_preview(*delta),
            Self::SetTabBarState(state) => app.set_tabbar_state(state),
            Self::NextTab => app.next_tab(),
            Self::PrevTab => app.prev_tab(),
            Self::TogglePretty => app.toggle_pretty(),
            Self::ToggleDecode => app.toggle_decode(),
            Self::ToggleHighlight => app.toggle_highlight(),
            Self::StartInput(kind) => app.start_input(*kind),
            Self::NextMatch => app.jump_to_match(MatchJump::Next),
            Self::PrevMatch => app.jump_to_match(MatchJump::Prev),
            Self::ClearSearch => app.clear_search(),
            Self::InputChar(c) => app.input.insert(*c),
            Self::InputBackspace => app.input.backspace(),
            Self::InputDelete => app.input.delete(),
            Self::InputLeft => app.input.left(),
            Self::InputRight => app.input.right(),
            Self::InputHome => app.input.home(),
            Self::InputEnd => app.input.end(),
            Self::InputClear => app.input.clear(),
            Self::InputSubmit => app.submit_input(),
            Self::InputCancel => app.cancel_input(),
            Self::OpenExport => app.open_export(),
            Self::ExportSelect(target) => app.select_export_target(*target),
            Self::ExportCycle(delta) => app.cycle_export_target(*delta),
            Self::ExportToggleDecoded => app.toggle_export_decoded(),
            Self::ExportToFile => app.export_to_file(),
            Self::ExportToClipboard => app.export_to_clipboard(),
            Self::ExportCancel => app.close_export(),
        }
    }
}

/// Key bindings shown on the Help tab, grouped by section.
/// Keep in sync with `normal_mode_keys`.
pub const KEY_HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Entries",
        &[
            ("j / k, Down / Up", "Move focus down / up"),
            ("d / u", "Move focus down / up faster"),
            ("g / G", "Jump to first / last entry"),
        ],
    ),
    (
        "Preview",
        &[
            ("J / K", "Scroll preview down / up"),
            ("PgDn / PgUp", "Scroll preview a page down / up"),
            ("1 - 4", "Headers, Cookies, Request, Response tab"),
            ("5 / ?", "Help tab"),
            ("Tab / Shift-Tab", "Next / previous tab"),
        ],
    ),
    (
        "Body",
        &[
            ("p", "Toggle pretty printing (JSON, XML, HTML)"),
            ("x", "Toggle decoding: base64 bodies, URL / base64 form and query values"),
            ("s", "Toggle syntax highlighting (JSON/XML to 200KB, JS/CSS/HTML to 32KB)"),
        ],
    ),
    (
        "Search",
        &[
            ("/", "Filter entries (query syntax below)"),
            ("f", "Find in preview, without filtering"),
            ("n / N", "Next / previous match in preview"),
            ("Esc", "Clear find, then filter"),
            ("Enter / Esc", "In the input bar: apply / cancel"),
            ("Ctrl-U", "In the input bar: clear"),
        ],
    ),
    (
        "Query syntax (terms are ANDed, ! negates)",
        &[
            ("token", "Regex in URL, headers, params, bodies"),
            ("url: host:", "Regex on the URL / host name"),
            ("header:", "Regex on request / response headers"),
            ("param:", "Regex on decoded query params / form fields"),
            ("body:", "Regex on decoded request / response bodies"),
            ("type:", "Regex on the response content type"),
            ("method:POST", "Request method (exact)"),
            ("status:", "404, 4xx, >=500"),
            ("size:", ">100k, <=2mb (response size)"),
            ("dur:", ">500ms, <2s (total time)"),
            ("\"a b\"", "Quote terms containing spaces"),
            ("Case", "Insensitive unless the pattern has uppercase"),
        ],
    ),
    (
        "Export",
        &[
            ("e", "Export menu: request / response body, full request / response"),
            ("1 - 4, j / k", "In the menu: choose what to export"),
            ("d", "In the menu: decoded (as previewed) or as stored in the HAR"),
            ("Enter / f", "In the menu: save to a file (default under exports/)"),
            ("c", "In the menu: copy to the clipboard (OSC 52, up to 100KB)"),
        ],
    ),
    ("General", &[("q / Ctrl-C", "Quit")]),
];

pub fn handle_key_events(key_event: KeyEvent, mode: Mode) -> Option<Command> {
    match mode {
        Mode::Normal => normal_mode_keys(key_event),
        Mode::Input(_) => input_mode_keys(key_event),
        Mode::ExportMenu => export_menu_keys(key_event),
    }
}

fn export_menu_keys(key_event: KeyEvent) -> Option<Command> {
    let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
    match key_event.code {
        KeyCode::Char('c') if ctrl => Some(Command::Quit),
        KeyCode::Char(digit @ '1'..='4') => {
            let index = digit as usize - '1' as usize;
            Some(Command::ExportSelect(ExportTarget::ALL[index]))
        }
        KeyCode::Char('j') | KeyCode::Down => Some(Command::ExportCycle(1)),
        KeyCode::Char('k') | KeyCode::Up => Some(Command::ExportCycle(-1)),
        KeyCode::Char('d') => Some(Command::ExportToggleDecoded),
        KeyCode::Enter | KeyCode::Char('f') => Some(Command::ExportToFile),
        KeyCode::Char('c') => Some(Command::ExportToClipboard),
        KeyCode::Esc | KeyCode::Char('q') => Some(Command::ExportCancel),
        _ => None,
    }
}

fn input_mode_keys(key_event: KeyEvent) -> Option<Command> {
    let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);
    match key_event.code {
        KeyCode::Char('c') if ctrl => Some(Command::Quit),
        KeyCode::Char('u') if ctrl => Some(Command::InputClear),
        KeyCode::Char(c) if !ctrl => Some(Command::InputChar(c)),
        KeyCode::Backspace => Some(Command::InputBackspace),
        KeyCode::Delete => Some(Command::InputDelete),
        KeyCode::Left => Some(Command::InputLeft),
        KeyCode::Right => Some(Command::InputRight),
        KeyCode::Home => Some(Command::InputHome),
        KeyCode::End => Some(Command::InputEnd),
        KeyCode::Enter => Some(Command::InputSubmit),
        KeyCode::Esc => Some(Command::InputCancel),
        _ => None,
    }
}

fn normal_mode_keys(key_event: KeyEvent) -> Option<Command> {
    match key_event.code {
        KeyCode::Char('q') => Some(Command::Quit),
        KeyCode::Char('c') | KeyCode::Char('C') => {
            if key_event.modifiers == KeyModifiers::CONTROL {
                Some(Command::Quit)
            } else {
                None
            }
        }
        KeyCode::Char('j') | KeyCode::Down => Some(Command::TableFocusDelta(1)),
        KeyCode::Char('k') | KeyCode::Up => Some(Command::TableFocusDelta(-1)),
        KeyCode::Char('d') => Some(Command::TableFocusDelta(3)),
        KeyCode::Char('u') => Some(Command::TableFocusDelta(-3)),
        KeyCode::Char('g') => Some(Command::TableFocusTop),
        KeyCode::Char('G') => Some(Command::TableFocusBottom),
        KeyCode::Char('J') => Some(Command::PreviewScroll(1)),
        KeyCode::Char('K') => Some(Command::PreviewScroll(-1)),
        KeyCode::PageDown => Some(Command::PreviewScroll(10)),
        KeyCode::PageUp => Some(Command::PreviewScroll(-10)),
        KeyCode::Char('1') => Some(Command::SetTabBarState(app::TabBarState::Headers)),
        KeyCode::Char('2') => Some(Command::SetTabBarState(app::TabBarState::Cookies)),
        KeyCode::Char('3') => Some(Command::SetTabBarState(app::TabBarState::Request)),
        KeyCode::Char('4') => Some(Command::SetTabBarState(app::TabBarState::Response)),
        KeyCode::Char('5') | KeyCode::Char('?') => {
            Some(Command::SetTabBarState(app::TabBarState::Help))
        }
        KeyCode::Tab => Some(Command::NextTab),
        KeyCode::BackTab => Some(Command::PrevTab),
        KeyCode::Char('p') => Some(Command::TogglePretty),
        KeyCode::Char('x') => Some(Command::ToggleDecode),
        KeyCode::Char('s') => Some(Command::ToggleHighlight),
        KeyCode::Char('e') => Some(Command::OpenExport),
        KeyCode::Char('/') => Some(Command::StartInput(InputKind::Filter)),
        KeyCode::Char('f') => Some(Command::StartInput(InputKind::Find)),
        KeyCode::Char('n') => Some(Command::NextMatch),
        KeyCode::Char('N') => Some(Command::PrevMatch),
        KeyCode::Esc => Some(Command::ClearSearch),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn handle_key_events_question_mark_selects_help_tab() {
        let command = handle_key_events(key(KeyCode::Char('?')), Mode::Normal);

        assert_eq!(command, Some(Command::SetTabBarState(app::TabBarState::Help)));
    }

    #[test]
    fn handle_key_events_tab_selects_next_tab() {
        let command = handle_key_events(key(KeyCode::Tab), Mode::Normal);

        assert_eq!(command, Some(Command::NextTab));
    }

    #[test]
    fn handle_key_events_shift_tab_selects_previous_tab() {
        let shift_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);

        let command = handle_key_events(shift_tab, Mode::Normal);

        assert_eq!(command, Some(Command::PrevTab));
    }

    #[test]
    fn handle_key_events_x_toggles_decode() {
        let command = handle_key_events(key(KeyCode::Char('x')), Mode::Normal);

        assert_eq!(command, Some(Command::ToggleDecode));
    }

    #[test]
    fn handle_key_events_p_toggles_pretty() {
        let command = handle_key_events(key(KeyCode::Char('p')), Mode::Normal);

        assert_eq!(command, Some(Command::TogglePretty));
    }

    const FILTER_INPUT: Mode = Mode::Input(InputKind::Filter);

    #[test]
    fn handle_key_events_slash_starts_filter_input() {
        let command = handle_key_events(key(KeyCode::Char('/')), Mode::Normal);

        assert_eq!(command, Some(Command::StartInput(InputKind::Filter)));
    }

    #[test]
    fn handle_key_events_f_starts_find_input() {
        let command = handle_key_events(key(KeyCode::Char('f')), Mode::Normal);

        assert_eq!(command, Some(Command::StartInput(InputKind::Find)));
    }

    #[test]
    fn handle_key_events_in_input_mode_types_normal_mode_keys() {
        // `q` quits in normal mode but must be typeable in a query
        let command = handle_key_events(key(KeyCode::Char('q')), FILTER_INPUT);

        assert_eq!(command, Some(Command::InputChar('q')));
    }

    #[test]
    fn handle_key_events_in_input_mode_types_shifted_characters() {
        let shifted = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);

        let command = handle_key_events(shifted, FILTER_INPUT);

        assert_eq!(command, Some(Command::InputChar('N')));
    }

    #[test]
    fn handle_key_events_in_input_mode_esc_cancels() {
        let command = handle_key_events(key(KeyCode::Esc), FILTER_INPUT);

        assert_eq!(command, Some(Command::InputCancel));
    }

    #[test]
    fn handle_key_events_in_input_mode_ctrl_c_quits() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let command = handle_key_events(ctrl_c, FILTER_INPUT);

        assert_eq!(command, Some(Command::Quit));
    }

    #[test]
    fn handle_key_events_s_toggles_highlight() {
        let command = handle_key_events(key(KeyCode::Char('s')), Mode::Normal);

        assert_eq!(command, Some(Command::ToggleHighlight));
    }

    #[test]
    fn handle_key_events_e_opens_export() {
        let command = handle_key_events(key(KeyCode::Char('e')), Mode::Normal);

        assert_eq!(command, Some(Command::OpenExport));
    }

    #[test]
    fn handle_key_events_in_export_menu_digits_select_target() {
        let command = handle_key_events(key(KeyCode::Char('3')), Mode::ExportMenu);

        assert_eq!(command, Some(Command::ExportSelect(ExportTarget::FullRequest)));
    }

    #[test]
    fn handle_key_events_in_export_menu_c_copies_and_q_cancels() {
        let commands = (
            handle_key_events(key(KeyCode::Char('c')), Mode::ExportMenu),
            handle_key_events(key(KeyCode::Char('q')), Mode::ExportMenu),
        );

        assert_eq!(
            commands,
            (Some(Command::ExportToClipboard), Some(Command::ExportCancel))
        );
    }

    #[test]
    fn handle_key_events_ctrl_c_quits() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let command = handle_key_events(ctrl_c, Mode::Normal);

        assert_eq!(command, Some(Command::Quit));
    }

    #[test]
    fn handle_key_events_plain_c_does_nothing() {
        let command = handle_key_events(key(KeyCode::Char('c')), Mode::Normal);

        assert_eq!(command, None);
    }
}
