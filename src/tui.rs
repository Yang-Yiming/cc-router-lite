use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
    DefaultTerminal, Frame, TerminalOptions, Viewport,
};

use crate::config::{parse_hex_color, Target};
use crate::error::CcrlError;

const MIN_VIEWPORT_HEIGHT: u16 = 10;
const MAX_VIEWPORT_HEIGHT: u16 = 18;
const CHROME_HEIGHT: u16 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiProfileItem {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiSelection {
    pub target: Target,
    pub profile: String,
}

#[derive(Debug)]
struct TuiColumn {
    target: Target,
    items: Vec<TuiProfileItem>,
    selected: usize,
}

impl TuiColumn {
    fn new(target: Target, items: Vec<TuiProfileItem>) -> Self {
        let selected = items.iter().position(|item| item.active).unwrap_or(0);
        Self {
            target,
            items,
            selected,
        }
    }

    fn selected_item(&self) -> Option<&TuiProfileItem> {
        self.items.get(self.selected)
    }

    fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.items.len() - 1);
    }
}

#[derive(Debug)]
struct TuiApp {
    focused_target: Target,
    claude: TuiColumn,
    codex: TuiColumn,
}

impl TuiApp {
    fn new(
        focused_target: Target,
        claude_items: Vec<TuiProfileItem>,
        codex_items: Vec<TuiProfileItem>,
    ) -> Self {
        Self {
            focused_target,
            claude: TuiColumn::new(Target::Claude, claude_items),
            codex: TuiColumn::new(Target::Codex, codex_items),
        }
    }

    fn viewport_height(&self) -> u16 {
        let max_items = self.claude.items.len().max(self.codex.items.len()) as u16;
        (max_items + CHROME_HEIGHT).clamp(MIN_VIEWPORT_HEIGHT, MAX_VIEWPORT_HEIGHT)
    }

    fn current_column(&self) -> &TuiColumn {
        match self.focused_target {
            Target::Claude => &self.claude,
            Target::Codex => &self.codex,
        }
    }

    fn current_column_mut(&mut self) -> &mut TuiColumn {
        match self.focused_target {
            Target::Claude => &mut self.claude,
            Target::Codex => &mut self.codex,
        }
    }

    fn set_focus(&mut self, target: Target) {
        self.focused_target = target;
    }

    fn toggle_focus(&mut self) {
        self.focused_target = match self.focused_target {
            Target::Claude => Target::Codex,
            Target::Codex => Target::Claude,
        };
    }

    fn activate(&self) -> Option<TuiSelection> {
        self.current_column()
            .selected_item()
            .map(|item| TuiSelection {
                target: self.current_column().target,
                profile: item.name.clone(),
            })
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Option<TuiSelection> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match key.code {
            KeyCode::Tab => {
                self.toggle_focus();
                None
            }
            KeyCode::Left => {
                self.set_focus(Target::Claude);
                None
            }
            KeyCode::Right => {
                self.set_focus(Target::Codex);
                None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.current_column_mut().move_down();
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.current_column_mut().move_up();
                None
            }
            KeyCode::Enter => self.activate(),
            KeyCode::Esc | KeyCode::Char('q') => Some(TuiSelection {
                target: self.focused_target,
                profile: String::new(),
            }),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TuiSelection {
                    target: self.focused_target,
                    profile: String::new(),
                })
            }
            _ => None,
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let [tabs_area, lists_area, help_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        self.render_tabs(frame, tabs_area);

        let [claude_area, codex_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(lists_area);
        Self::render_column(
            frame,
            claude_area,
            &self.claude,
            self.focused_target == Target::Claude,
        );
        Self::render_column(
            frame,
            codex_area,
            &self.codex,
            self.focused_target == Target::Codex,
        );

        let help = Paragraph::new(
            "Tab/←/→ switch column  j/k/↑/↓ move  Enter activate  Esc/q/Ctrl-C quit",
        )
        .style(Style::default().fg(Color::DarkGray))
        .centered();
        frame.render_widget(help, help_area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let tabs = Tabs::new(vec!["Claude", "Codex"])
            .select(match self.focused_target {
                Target::Claude => 0,
                Target::Codex => 1,
            })
            .block(Block::default().borders(Borders::ALL).title("Targets"))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().fg(Color::Gray))
            .divider(" ")
            .padding(" ", " ");
        frame.render_widget(tabs, area);
    }

    fn render_column(
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        column: &TuiColumn,
        focused: bool,
    ) {
        let items: Vec<ListItem> = column.items.iter().map(render_item).collect();
        let title = match column.target {
            Target::Claude => "Claude",
            Target::Codex => "Codex",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(if focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            });

        let list = if items.is_empty() {
            List::new(vec![ListItem::new(Line::from("  No profiles"))])
        } else {
            List::new(items)
        }
        .block(block)
        .highlight_style(
            Style::default()
                .bg(if focused {
                    Color::DarkGray
                } else {
                    Color::Black
                })
                .add_modifier(Modifier::BOLD),
        );

        let mut state = if column.items.is_empty() {
            ListState::default()
        } else {
            ListState::default().with_selected(Some(column.selected))
        };
        frame.render_stateful_widget(list, area, &mut state);
    }
}

pub fn run(
    focused_target: Target,
    claude_items: Vec<TuiProfileItem>,
    codex_items: Vec<TuiProfileItem>,
) -> Result<Option<TuiSelection>, CcrlError> {
    let mut app = TuiApp::new(focused_target, claude_items, codex_items);
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(app.viewport_height()),
    })?;
    let result = run_app(&mut terminal, &mut app);
    let restore_result = ratatui::try_restore().map_err(CcrlError::from);

    match (result, restore_result) {
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Ok(selection), Ok(())) => Ok(selection),
    }
}

fn run_app(
    terminal: &mut DefaultTerminal,
    app: &mut TuiApp,
) -> Result<Option<TuiSelection>, CcrlError> {
    loop {
        terminal.draw(|frame| app.render(frame))?;
        if let Event::Key(key) = event::read()? {
            if let Some(selection) = app.handle_key_event(key) {
                return if selection.profile.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(selection))
                };
            }
        }
    }
}

fn render_item(item: &TuiProfileItem) -> ListItem<'static> {
    let mut spans = vec![Span::styled(
        if item.active { "* " } else { "  " },
        Style::default().fg(if item.active {
            Color::Cyan
        } else {
            Color::DarkGray
        }),
    )];

    let mut name_style = Style::default();
    if let Some(color) = profile_color(item.color.as_deref()) {
        name_style = name_style.fg(color);
    }
    if item.active {
        name_style = name_style.add_modifier(Modifier::BOLD);
    }
    spans.push(Span::styled(item.name.clone(), name_style));

    if let Some(description) = &item.description {
        spans.push(Span::styled(
            format!(" - {description}"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn profile_color(color: Option<&str>) -> Option<Color> {
    match color {
        Some("red") => Some(Color::Red),
        Some("green") => Some(Color::Green),
        Some("yellow") => Some(Color::Yellow),
        Some("blue") => Some(Color::Blue),
        Some("magenta") => Some(Color::Magenta),
        Some("cyan") => Some(Color::Cyan),
        Some("white") => Some(Color::White),
        Some("black") => Some(Color::Black),
        Some(other) => parse_hex_color(other).map(|(r, g, b)| Color::Rgb(r, g, b)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> TuiProfileItem {
        TuiProfileItem {
            name: name.to_string(),
            description: None,
            color: None,
            active: false,
        }
    }

    #[test]
    fn tab_switch_preserves_each_column_selection() {
        let mut app = TuiApp::new(
            Target::Claude,
            vec![item("a"), item("b"), item("c")],
            vec![item("x"), item("y"), item("z")],
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert_eq!(app.focused_target, Target::Claude);
        assert_eq!(app.claude.selected, 2);
        assert_eq!(app.codex.selected, 1);
    }

    #[test]
    fn horizontal_keys_switch_focus_without_moving_selection() {
        let mut app = TuiApp::new(Target::Claude, vec![item("a"), item("b")], vec![item("x")]);
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        app.handle_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.focused_target, Target::Codex);
        assert_eq!(app.claude.selected, 1);

        app.handle_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.focused_target, Target::Claude);
        assert_eq!(app.claude.selected, 1);
    }

    #[test]
    fn enter_activates_focused_column_selection() {
        let mut app = TuiApp::new(
            Target::Codex,
            vec![item("a")],
            vec![item("OAuth"), item("work")],
        );
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let selection = app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(
            selection,
            Some(TuiSelection {
                target: Target::Codex,
                profile: "work".to_string(),
            })
        );
    }

    #[test]
    fn active_item_is_initial_selection_for_each_column() {
        let mut active_claude = item("active-claude");
        active_claude.active = true;
        let mut active_codex = item("OAuth");
        active_codex.active = true;

        let app = TuiApp::new(
            Target::Claude,
            vec![item("a"), active_claude],
            vec![active_codex, item("work")],
        );

        assert_eq!(app.claude.selected, 1);
        assert_eq!(app.codex.selected, 0);
    }
}
