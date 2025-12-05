use crossterm::event::{self, Event, KeyCode};
use listeners::get_all;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};
use std::io;
use sysinfo::{Pid, System};

#[derive(Debug, Clone)]
struct PortData {
    port: u16,
    pid: u32,
    name: String,
    command: String,
}

#[derive(PartialEq)]
enum AppMode {
    Normal,
    Confirming,
    Searching,
}
struct App {
    displayed_ports: Vec<PortData>,
    raw_ports: Vec<PortData>,
    state: TableState,
    system: System,
    should_quit: bool,
    mode: AppMode,
    status_feedback: Option<(String, Color)>,
    search_query: String,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            displayed_ports: vec![],
            raw_ports: vec![],
            state: TableState::default(),
            system: System::new_all(),
            should_quit: false,
            mode: AppMode::Normal,
            status_feedback: None,
            search_query: String::new(),
        };

        app.state.select(Some(0));
        app.refresh();
        app
    }

    fn run_filter(&mut self) {
        if self.search_query.is_empty() {
            self.displayed_ports = self.raw_ports.clone();
        } else {
            let query = self.search_query.to_lowercase();
            self.displayed_ports = self
                .raw_ports
                .iter()
                .filter(|p| {
                    p.name.to_lowercase().contains(&query)
                        || p.pid.to_string().contains(&query)
                        || p.port.to_string().contains(&query)
                })
                .cloned()
                .collect()
        }
        self.state.select(Some(0));
    }

    fn refresh(&mut self) {
        self.raw_ports.clear();
        self.system.refresh_all();

        if let Ok(listeners) = get_all() {
            for l in listeners {
                let pid_u32 = l.process.pid;
                let mut cmd_string = String::from("-");
                let mut proc_name = l.process.name.clone();

                if let Some(process) = self.system.process(Pid::from_u32(pid_u32)) {
                    if let Some(name) = process.name().to_str() {
                        proc_name = name.to_string();
                    }
                    if !process.cmd().is_empty() {
                        cmd_string = process
                            .cmd()
                            .iter()
                            .map(|s| s.to_string_lossy().to_string())
                            .collect::<Vec<String>>()
                            .join(" ");
                    }
                }
                self.raw_ports.push(PortData {
                    port: l.socket.port(),
                    pid: pid_u32,
                    name: proc_name,
                    command: cmd_string,
                })
            }
        }
        self.raw_ports.sort_by_key(|k| k.port);

        self.run_filter()
    }

    fn kill_selected(&mut self) {
        if let Some(selected_idx) = self.state.selected() {
            if selected_idx < self.displayed_ports.len() {
                let pid = self.displayed_ports[selected_idx].pid;

                if let Some(process) = self.system.process(Pid::from_u32(pid)) {
                    let status = process.kill();
                    if status {
                        self.status_feedback =
                            Some((format!("Successful removed PID {}", pid), Color::Green));
                    } else {
                        self.status_feedback =
                            Some((format!("Failed to kill PID {}", pid), Color::Red))
                    }
                } else {
                    self.status_feedback =
                        Some((format!("Process {} not found", pid), Color::Yellow));
                }
                self.refresh();
            }
        }
    }

    fn next_row(&mut self) {
        self.status_feedback = None;
        if self.displayed_ports.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.displayed_ports.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous_row(&mut self) {
        self.status_feedback = None;
        if self.displayed_ports.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.displayed_ports.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();

    let app_result = loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Down => app.next_row(),
                        KeyCode::Up => app.previous_row(),
                        KeyCode::Char('k') => app.mode = AppMode::Confirming,
                        KeyCode::Char('/') => {
                            app.mode = AppMode::Searching;
                        }
                        _ => {}
                    },
                    AppMode::Confirming => match key.code {
                        KeyCode::Char('y') => {
                            app.kill_selected();
                            app.mode = AppMode::Normal
                        }
                        KeyCode::Char('n') | KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char('q') => app.should_quit = true,
                        _ => {}
                    },
                    AppMode::Searching => match key.code {
                        KeyCode::Enter | KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.run_filter()
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.run_filter();
                        }
                        _ => {}
                    },
                }
            }
        }
        if app.should_quit {
            break Ok(());
        }
    };
    ratatui::restore();
    app_result
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(100 - percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(100 - percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn ui(f: &mut Frame, app: &mut App) {
    let rects = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).split(f.area());

    let rows: Vec<Row> = app
        .displayed_ports
        .iter()
        .map(|p| {
            Row::new(vec![
                Cell::from(p.port.to_string()).style(Style::default().fg(Color::Cyan)),
                Cell::from(p.pid.to_string()),
                Cell::from(p.name.clone()).style(Style::default().fg(Color::Green)),
                Cell::from(p.command.clone()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(20),
        Constraint::Min(20),
    ];

    let header = Row::new(vec!["PORT", "PID", "NAME", "COMMAND"])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(1);

    let title = if app.mode == AppMode::Searching || !app.search_query.is_empty() {
        format!(" Search: {}_ ", app.search_query)
    } else {
        String::from(" KillPort TUI ")
    };

    let title_style = if app.mode == AppMode::Searching {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(title, title_style)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(table, rects[0], &mut app.state);

    let (status_text, status_style) = match &app.status_feedback {
        Some((msg, color)) => (
            format!(" {} ", msg),
            Style::default()
                .bg(*color)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        None => (
            String::from(" Ready "),
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let footer_text = Line::from(vec![
        Span::styled(status_text, status_style),
        Span::raw(" "),
        Span::styled(
            " k ",
            Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Kill Port "),
        Span::raw(" | "),
        Span::styled(
            " q ",
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Quit "),
        Span::raw(" | "),
        Span::raw(" ↑/↓ Navigate "),
        Span::raw(" |  "),
        Span::raw(" / Search"),
    ]);

    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(footer, rects[1]);

    if app.mode == AppMode::Confirming {
        let block = Block::default()
            .title(" Confirm ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray));

        let area = centered_rect(40, 20, f.area());

        let text = Paragraph::new(vec![
            Line::from("Are you sure you want to kill this process?"),
            Line::from(""),
            Line::from(vec![
                Span::styled(" (y)", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Yes.   "),
                Span::styled("(n)", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" No."),
            ]),
        ])
        .block(block)
        .alignment(Alignment::Center);

        f.render_widget(Clear, area);
        f.render_widget(text, area);
    }
}
