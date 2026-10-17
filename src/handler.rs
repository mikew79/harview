use crate::app::{self, Mode};
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
}

impl Command {
    pub fn exec(&self, app: &mut app::App) {
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
        ],
    ),
    ("General", &[("q / Ctrl-C", "Quit")]),
];

pub fn handle_key_events(key_event: KeyEvent, mode: Mode) -> Option<Command> {
    match mode {
        Mode::Normal => normal_mode_keys(key_event),
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
