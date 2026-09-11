use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table, TableState};
use ratatui::{Frame, Terminal};
use std::io;

use crate::example::Example;
use crate::manpage::ManOption;

/// A displayable item that can be either an option or an example
#[derive(Clone, Debug)]
pub enum TuiItem {
    Option(ManOption),
    Example(Example),
}

impl TuiItem {
    pub fn display_text(&self) -> String {
        match self {
            TuiItem::Option(opt) => opt.display_string(),
            TuiItem::Example(ex) => ex.description.clone(),
        }
    }

    pub fn command_line(&self) -> Option<String> {
        match self {
            TuiItem::Option(opt) => {
                let flag = opt.long.as_ref().or_else(|| opt.short.as_ref())?;
                Some(format!("{}", flag))
            }
            TuiItem::Example(ex) => Some(ex.command_line.clone()),
        }
    }

    pub fn category(&self) -> String {
        match self {
            TuiItem::Option(_) => "option".to_string(),
            TuiItem::Example(_) => "example".to_string(),
        }
    }
}

/// App state for the TUI
pub struct App {
    pub tool_name: String,
    pub items: Vec<TuiItem>,
    pub filtered_items: Vec<(usize, i64)>, // (original index, score) from fuzzy matching
    pub table_state: TableState,
    pub search_query: String,
    pub search_mode: bool,
    pub sort_mode: SortMode,
    pub scroll_detail: u16,
    pub copied_message: Option<String>,
    pub clipboard: Option<Clipboard>,
    pub matcher: SkimMatcherV2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortMode {
    Relevance,
    NameAsc,
    NameDesc,
}

impl App {
    pub fn new(tool_name: &str, options: Vec<ManOption>, examples: Vec<Example>) -> Self {
        let mut items = Vec::new();

        // Add options first
        for opt in options {
            items.push(TuiItem::Option(opt));
        }

        // Then add examples
        for ex in examples {
            items.push(TuiItem::Example(ex));
        }

        let filtered_items: Vec<(usize, i64)> = (0..items.len()).map(|i| (i, 0)).collect();

        let clipboard = Clipboard::new().ok();

        Self {
            tool_name: tool_name.to_string(),
            items,
            filtered_items,
            table_state: TableState::default(),
            search_query: String::new(),
            search_mode: false,
            sort_mode: SortMode::Relevance,
            scroll_detail: 0,
            copied_message: None,
            clipboard,
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn filter_items(&mut self) {
        let query = self.search_query.to_lowercase();

        if query.is_empty() {
            self.filtered_items = (0..self.items.len()).map(|i| (i, 0)).collect();
        } else {
            self.filtered_items = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let text = item.display_text().to_lowercase();
                    self.matcher.fuzzy_match(&text, &query).map(|score| (idx, score))
                })
                .collect();
        }

        // Apply sorting
        match self.sort_mode {
            SortMode::NameAsc => {
                self.filtered_items.sort_by_key(|(idx, _)| {
                    self.items[*idx].display_text().to_lowercase()
                });
            }
            SortMode::NameDesc => {
                self.filtered_items.sort_by_key(|(idx, _)| {
                    self.items[*idx].display_text().to_lowercase()
                });
                self.filtered_items.reverse();
            }
            SortMode::Relevance => {
                if !query.is_empty() {
                    self.filtered_items.sort_by(|a, b| b.1.cmp(&a.1));
                }
            }
        }

        // Reset selection if out of bounds
        let count = self.filtered_items.len();
        if count > 0 {
            let current = self.table_state.selected().unwrap_or(0);
            if current >= count {
                self.table_state.select(Some(count - 1));
            }
        } else {
            self.table_state.select(None);
        }
    }

    pub fn next_item(&mut self) {
        let count = self.filtered_items.len();
        if count == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= count - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
        self.scroll_detail = 0;
    }

    pub fn previous_item(&mut self) {
        let count = self.filtered_items.len();
        if count == 0 {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    count - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
        self.scroll_detail = 0;
    }

    pub fn first_item(&mut self) {
        if !self.filtered_items.is_empty() {
            self.table_state.select(Some(0));
            self.scroll_detail = 0;
        }
    }

    pub fn last_item(&mut self) {
        let count = self.filtered_items.len();
        if count > 0 {
            self.table_state.select(Some(count - 1));
            self.scroll_detail = 0;
        }
    }

    pub fn toggle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Relevance => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::Relevance,
        };
        self.filter_items();
    }

    pub fn toggle_search(&mut self) {
        self.search_mode = !self.search_mode;
        if !self.search_mode {
            self.search_query.clear();
            self.filter_items();
        }
    }

    pub fn append_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.filter_items();
    }

    pub fn backspace_search(&mut self) {
        self.search_query.pop();
        self.filter_items();
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.filter_items();
    }

    pub fn copy_selected(&mut self) {
        if let Some(idx) = self.table_state.selected() {
            if let Some((original_idx, _)) = self.filtered_items.get(idx) {
                if let Some(cmd) = self.items[*original_idx].command_line() {
                    if let Some(ref mut clipboard) = self.clipboard {
                        if clipboard.set_text(cmd.clone()).is_ok() {
                            self.copied_message = Some(format!("Copied: {}", cmd));
                        }
                    } else {
                        self.copied_message = Some("Clipboard unavailable (no display?)".to_string());
                    }
                }
            }
        }
    }

    pub fn get_selected_item(&self) -> Option<&TuiItem> {
        self.table_state.selected().and_then(|idx| {
            self.filtered_items.get(idx).map(|(original_idx, _)| &self.items[*original_idx])
        })
    }

    pub fn option_count(&self) -> usize {
        self.items.iter().filter(|i| matches!(i, TuiItem::Option(_))).count()
    }

    pub fn example_count(&self) -> usize {
        self.items.iter().filter(|i| matches!(i, TuiItem::Example(_))).count()
    }
}

/// Run the TUI
pub fn run_tui(
    tool_name: &str,
    options: Vec<ManOption>,
    examples: Vec<Example>,
) -> Result<()> {
    enable_raw_mode()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(tool_name, options, examples);
    if !app.filtered_items.is_empty() {
        app.table_state.select(Some(0));
    }

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    terminal.clear()?;

    result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_millis(250);

    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        // Quit
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if app.search_mode {
                                app.toggle_search();
                            } else {
                                return Ok(());
                            }
                        }

                        // Navigation
                        KeyCode::Down | KeyCode::Char('j') => app.next_item(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous_item(),
                        KeyCode::Char('g') => app.first_item(),
                        KeyCode::Char('G') => app.last_item(),
                        KeyCode::PageDown => {
                            for _ in 0..10 {
                                app.next_item();
                            }
                        }
                        KeyCode::PageUp => {
                            for _ in 0..10 {
                                app.previous_item();
                            }
                        }

                        // Search
                        KeyCode::Char('/') => app.toggle_search(),
                        KeyCode::Enter => {
                            if app.search_mode {
                                app.toggle_search();
                            }
                        }

                        // Sort
                        KeyCode::Char('s') => app.toggle_sort(),

                        // Copy
                        KeyCode::Char('c') | KeyCode::Char('y') => app.copy_selected(),

                        // Detail scroll
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.scroll_detail = app.scroll_detail.saturating_add(1);
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.scroll_detail = app.scroll_detail.saturating_sub(1);
                        }

                        // Search input
                        KeyCode::Char(c) => {
                            if app.search_mode {
                                app.append_search_char(c);
                            }
                        }
                        KeyCode::Backspace => {
                            if app.search_mode {
                                app.backspace_search();
                            }
                        }
                        KeyCode::Delete => {
                            if app.search_mode {
                                app.clear_search();
                            }
                        }

                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = std::time::Instant::now();
        }
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Search bar
            Constraint::Min(10),   // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_search_bar(f, app, chunks[1]);
    draw_main_content(f, app, chunks[2]);
    draw_status_bar(f, app, chunks[3]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" ex-man → {} ", app.tool_name);
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("ex-man", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" → "),
            Span::styled(
                &app.tool_name,
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{} options", app.option_count()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" | "),
            Span::styled(
                format!("{} examples", app.example_count()),
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "q:quit  /:search  s:sort  c/y:copy  ↑↓:nav  h/l:scroll detail",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title)
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .alignment(Alignment::Left);

    f.render_widget(header, area);
}

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let search_text = if app.search_mode {
        format!("> {}_", app.search_query)
    } else {
        format!("  {}", app.search_query)
    };

    let border_style = if app.search_mode {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search = Paragraph::new(search_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .title(if app.search_mode {
                    " Search (Enter to confirm, Esc to cancel) "
                } else {
                    " Search (press / to search) "
                })
                .border_style(border_style),
        )
        .style(Style::default().fg(if app.search_mode {
            Color::Yellow
        } else {
            Color::Gray
        }));

    f.render_widget(search, area);
}

fn draw_main_content(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    draw_item_list(f, app, chunks[0]);
    draw_detail_panel(f, app, chunks[1]);
}

fn draw_item_list(f: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(vec!["Type", "Flag/Name", "Description"])
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let rows: Vec<Row> = app
        .filtered_items
        .iter()
        .enumerate()
        .map(|(display_idx, (original_idx, _score))| {
            let item = &app.items[*original_idx];
            let is_selected = app.table_state.selected() == Some(display_idx);

            let (type_cell, flag_cell, desc_cell) = match item {
                TuiItem::Option(opt) => {
                    let flag = opt.display_string();
                    let flag_styled = Span::styled(flag, Style::default().fg(Color::Green));
                    let type_styled = Span::styled("OPT", Style::default().fg(Color::Blue));
                    let desc_styled = Span::styled(
                        truncate(&opt.description, 40),
                        Style::default().fg(Color::Gray),
                    );
                    (type_styled, flag_styled, desc_styled)
                }
                TuiItem::Example(ex) => {
                    let flags = ex.flags_used.join(", ");
                    let flag_styled = Span::styled(
                        if flags.is_empty() { "combo".to_string() } else { flags },
                        Style::default().fg(Color::Magenta),
                    );
                    let type_styled = Span::styled("EX", Style::default().fg(Color::Yellow));
                    let desc_styled = Span::styled(
                        truncate(&ex.description, 40),
                        Style::default().fg(Color::Gray),
                    );
                    (type_styled, flag_styled, desc_styled)
                }
            };

            let row_style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            Row::new(vec![
                Line::from(type_cell),
                Line::from(flag_cell),
                Line::from(desc_cell),
            ])
            .style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Items ({}/{}) ", app.filtered_items.len(), app.items.len()))
            .title_style(Style::default().fg(Color::Cyan)),
    )
    .highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(">> ");

    let mut state = app.table_state.clone();
    f.render_stateful_widget(table, area, &mut state);

    // Render scrollbar
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None);
    let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(app.filtered_items.len())
        .position(app.table_state.selected().unwrap_or(0));
    f.render_stateful_widget(
        scrollbar,
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut scrollbar_state,
    );
}

fn draw_detail_panel(f: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(item) = app.get_selected_item() {
        match item {
            TuiItem::Option(opt) => {
                let flag_display = opt.display_string();
                let arg_info = if opt.takes_arg {
                    format!(" Takes argument: {}", opt.arg_name.as_deref().unwrap_or("ARG"))
                } else {
                    " No argument".to_string()
                };

                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Option: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                        Span::styled(flag_display.clone(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![
                        Span::styled("Section: ", Style::default().fg(Color::Cyan)),
                        Span::raw(opt.section.clone()),
                    ]),
                    Line::from(vec![
                        Span::styled("Argument: ", Style::default().fg(Color::Cyan)),
                        Span::raw(arg_info.clone()),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Description:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ]),
                ];

                // Wrap description
                let wrapped = textwrap::wrap(&opt.description, (area.width as usize).saturating_sub(4));
                for line in wrapped {
                    lines.push(Line::from(line.to_string()));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Example command:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]));
                let cmd = format!("{} {}", app.tool_name, flag_display);
                lines.push(Line::from(vec![
                    Span::styled("$ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(cmd, Style::default().fg(Color::Yellow)),
                ]));

                lines
            }
            TuiItem::Example(ex) => {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled("Example: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                        Span::styled(&ex.description, Style::default().fg(Color::White)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Command:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![
                        Span::styled("$ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(&ex.command_line, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    ]),
                ];

                if !ex.flags_used.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("Flags used:", Style::default().fg(Color::Cyan)),
                    ]));
                    for flag in &ex.flags_used {
                        lines.push(Line::from(vec![
                            Span::styled("  • ", Style::default().fg(Color::DarkGray)),
                            Span::styled(flag, Style::default().fg(Color::Green)),
                        ]));
                    }
                }

                lines
            }
        }
    } else {
        vec![Line::from("No item selected")]
    };

    let detail = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Detail ")
                .title_style(Style::default().fg(Color::Cyan)),
        )
        .scroll((app.scroll_detail, 0));

    f.render_widget(detail, area);

    // Show copied message overlay
    if let Some(ref msg) = app.copied_message {
        let popup_area = centered_rect(50, 20, area);
        let popup = Paragraph::new(msg.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Copied! ")
                    .title_style(Style::default().fg(Color::Green)),
            )
            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        f.render_widget(Clear, popup_area);
        f.render_widget(popup, popup_area);
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let sort_label = match app.sort_mode {
        SortMode::Relevance => "relevance",
        SortMode::NameAsc => "name ↑",
        SortMode::NameDesc => "name ↓",
    };

    let status = format!(
        " Sort: {} | Filtered: {}/{} | Mode: {} ",
        sort_label,
        app.filtered_items.len(),
        app.items.len(),
        if app.search_mode { "SEARCH" } else { "NORMAL" }
    );

    let status_bar = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray).bg(Color::Black))
        .alignment(Alignment::Right);

    f.render_widget(status_bar, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}
