use std::collections::HashMap;

use crate::games::azul::ActionDisplay;
use crate::games::GameState;

use super::azul::{self, Tile, WALL_COLORS};
use ratatui::layout::{Constraint, Direction, Flex, Layout};
use ratatui::style::{self, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{BorderType, Borders, Cell, Clear, HighlightSpacing, List, Row, StatefulWidget, Table, TableState};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};

#[derive(Clone)]
pub struct Move {
    pub player: usize,
    pub action: azul::Action,
}

#[derive(Clone, Copy)]
pub struct ActionAnalysis {
    pub score_gain: i32,
    pub expected_score: Option<f32>,
    pub win_probability: Option<f32>,
}

#[derive(Clone)]
pub struct Heuristic {
    pub name: String,
    pub function: fn(&azul::State, usize) -> Option<azul::Action>,
}

#[derive(Clone)]
pub struct InteractiveApp {
    pub state: azul::State,
    pub current_player: usize,
    pub ply: usize,
    pub ply_round: usize,
    pub last_move: Option<Move>,
    pub actions: Vec<azul::Action>,
    pub actions_state: TableState,
    pub analyses: HashMap<azul::Action, ActionAnalysis>,
    pub show_action_details: bool,
    pub show_heuristic_details: bool,
    pub show_state_details: bool,
    pub heuristics: Vec<Heuristic>,
}

fn tile_to_color(tile: Tile) -> style::Color {
    match tile {
        Tile::Black => style::Color::Black,
        Tile::Blue => style::Color::Blue,
        Tile::Red => style::Color::Red,
        Tile::White => style::Color::White,
        Tile::Yellow => style::Color::Yellow,
    }
}

impl Widget for azul::CenterState {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut lines = Vec::new();

        let mut center_line = vec![" Center:".into()];
        if self.starting_marker {
            center_line.push(Span::styled(" 1", Style::default().fg(style::Color::Blue)));
        }

        if self.tiles.is_empty() {
            center_line.push(Span::styled(" ⬜", Style::default().fg(style::Color::Gray)));
        } else {
            for (&tile, &count) in self.tiles.iter() {
                for _ in 0..count {
                    center_line.push(Span::styled(" ⬛", Style::default().fg(tile_to_color(tile))));
                }
            }
        }

        lines.push(Line::from(center_line));

        Text::from(lines).render(area, buf);
    }
}

impl Widget for azul::PlayerState {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(format!("\n  Score: {}", self.score)).render(area, buf);

        let rows = 5;
        let cols = 5;
        let mut grid_lines = Vec::new();
        grid_lines.push(Line::from(""));
        grid_lines.push(Line::from(""));
        grid_lines.push(Line::from(""));

        for i in 0..rows {
            let mut row = vec![" ".into()];
            for j in 0..cols {
                let text = if j < (4 - i) {
                    Span::styled("   ", Style::default())
                } else {
                    match self.pattern_lines[i] {
                        (None, _) => {
                            Span::styled(" ⬜", Style::default().fg(style::Color::Gray))
                        },
                        (Some(tile), count) => {
                            let pos = 4 - j;
                            if pos < count {
                                Span::styled(" ⬛", Style::default().fg(tile_to_color(tile)))
                            } else {
                                Span::styled(" ⬜", Style::default().fg(style::Color::Gray))
                            }
                        }
                    }
                };
                row.push(text);
            }
            row.push("  ".into());

            for j in 0..cols {
                let text = if self.wall[i][j] { "⬛ " } else { "⬜ " };
                row.push(Span::styled(text, Style::default().fg(tile_to_color(WALL_COLORS[i][j]))));
            }
            grid_lines.push(Line::from(row));
        }

        grid_lines.push(Line::from(""));

        let mut row = vec![Span::styled(" ", Style::default())];
        if self.starting_marker {
            row.push(Span::styled(" 1", Style::default().fg(style::Color::Red)));
        }
        for i in 0..7 {
            if i < self.floor_line {
                row.push(Span::styled(" ⬤", Style::default().fg(style::Color::Red)));
            } else {
                row.push(Span::styled(" ⬤", Style::default().fg(style::Color::Gray)));
            }
        }
        grid_lines.push(Line::from(row));

        Text::from(grid_lines).render(area, buf);
    }
}

fn action_cell(action: &azul::Action) -> Cell {
    let display = match action.action_display_choice {
        ActionDisplay::FactoryDisplay(i) => format!("D{}", i),
        ActionDisplay::Center => "Center".to_string()
    };

    let row = match action.pattern_line_choice {
        Some(i) => format!("row {}", i),
        None => "penalty row".to_string()
    };

    Cell::from(Line::from(vec![
        display.into(),
        " ".into(),
        Span::styled("⬛", Style::default().fg(tile_to_color(action.color_choice))),
        " to ".into(),
        row.into()
    ]))
}

fn format_gain(gain: i32) -> Span<'static> {
    if gain == 0 {
        Span::styled("0", Style::default().blue())
    } else if gain.is_positive() {
        Span::styled(format!("+{}", gain), Style::default().green().reversed())
    } else {
        Span::styled(gain.to_string(), Style::default().red().reversed())
    }
 }

fn format_score(score: Option<f32>) -> Span<'static> {
    match score {
        Some(s) => Span::from(s.to_string()),
        None => Span::styled("NA", Style::default().gray()),
    }
}

impl Widget for InteractiveApp {
    fn render(mut self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Length(7),  // Displays
                Constraint::Length(12), // Player States
                Constraint::Length(15), // Actions
                Constraint::Length(7),  // Heuristics Analysis
                Constraint::Length(7),  // States Analysis
            ])
            .split(area);

        let block = Block::bordered()
            .border_set(border::THICK);

        let header_text = Text::from(vec![Line::from(vec![
            " ".into(),
            if self.state.is_game_over() {
                Span::styled(" GAME OVER ", Style::default().fg(style::Color::Red)).bold().add_modifier(Modifier::SLOW_BLINK | Modifier::REVERSED)
            } else {
                Span::styled(" GAME RUNNING ", Style::default().fg(style::Color::Blue)).bold().add_modifier(Modifier::REVERSED)
            },
            format!(" Players: {}, ", self.state.players.len()).into(),
            format!("Current Player: {}, ", self.current_player).into(),
            format!("Round: {}, ", self.state.rounds).into(),
            format!("Ply: {}, ({} this round)", self.ply, self.ply_round).into(),
        ])]);

        Paragraph::new(header_text)
            .block(block)
            .render(layout[0], buf);

        let display_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(4), Constraint::Length(3)])
            .split(layout[1]);

        let factory_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Length(9); self.state.factory_displays.len()])
            .split(display_layout[0]);

        for (i, fd) in self.state.factory_displays.iter().enumerate() {
            let mut lines = Vec::new();
            lines.push(Line::from(""));

            let n_tiles: usize = fd.values().sum();
            let mut tile_spans: Vec<Span> = Vec::with_capacity(4);

            for (&tile, &count) in fd.iter() {
                for _ in 0..count {
                    tile_spans.push(Span::styled("⬛ ", Style::default().fg(tile_to_color(tile))));
                }
            }

            for _ in 0..(4 - n_tiles) {
                tile_spans.push(Span::styled("⬜ ", Style::default().fg(style::Color::Gray)));
            }

            lines.push(Line::from(vec![
                "  ".into(),
                tile_spans[0].clone(),
                tile_spans[1].clone(),
            ]));
            lines.push(Line::from(vec![
                "  ".into(),
                tile_spans[2].clone(),
                tile_spans[3].clone(),
            ]));
            Text::from(lines).render(factory_layout[i], buf);

            Block::bordered().title(format!(" D{} ", i)).render(factory_layout[i], buf);
        }

        self.state.center.clone().render(display_layout[1], buf);

        let players_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Min(24); self.state.players.len()])
            .split(layout[2]);

        for i in 0..self.state.players.len() {
            let block = Block::default()
                .title(Line::from(format!(" Player {} ", i).bold()))
                .border_type(if self.current_player == i { BorderType::QuadrantOutside } else { BorderType::Plain })
                .border_style(Style::default().fg(style::Color::Blue))
                .borders(Borders::ALL);

            self.state.players[i].clone().render(players_layout[i], buf);
            block.render(players_layout[i], buf);
        }

        let actions_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Max(4), Constraint::Max(10)])
            .split(layout[3]);

        let mut last_move_lines = Vec::new();
        last_move_lines.push(Line::from(""));
        last_move_lines.push(Line::from(""));

        match self.last_move {
            Some(mov) => {
                let display = match mov.action.action_display_choice {
                    ActionDisplay::FactoryDisplay(i) => format!("D{}", i),
                    ActionDisplay::Center => "Center".to_string()
                };

                let row = match mov.action.pattern_line_choice {
                    Some(i) => format!("row {}", i),
                    None => "penalty row".to_string()
                };

                last_move_lines.push(Line::from(vec![
                    format!("        Last Move by P{}: ", mov.player).italic().into(),
                    display.into(),
                    " ".into(),
                    Span::styled("⬛", Style::default().fg(tile_to_color(mov.action.color_choice))),
                    " to ".into(),
                    row.into()
                ]));
            },
            None => {
                last_move_lines.push("        Last Move: NA".italic().into())
            }
        }

        Paragraph::new(last_move_lines)
            .render(actions_layout[0], buf);

        let mut rows: Vec<Row> = vec![];

        for (idx, action) in self.actions.iter().enumerate() {
            if self.analyses.contains_key(action) {
                let analysis = self.analyses[action];
                rows.push(Row::new(vec![
                    Cell::from(format!(" {:>3}. ", idx)),
                    action_cell(action),
                    Cell::from(format_gain(analysis.score_gain)),
                    Cell::from(format_score(analysis.expected_score)),
                    Cell::from(format_score(analysis.win_probability)),
                ]));
            } else {
                rows.push(Row::new(vec![
                    Cell::from(idx.to_string()),
                    action_cell(action),
                    Cell::from(format_score(None)),
                    Cell::from(format_score(None)),
                    Cell::from(format_score(None)),
                ]));
            }
        }

        let table = Table::new(rows, [
            Constraint::Length(6),
            Constraint::Percentage(30),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ])
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_symbol(" →")
            .row_highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .column_spacing(1)
            .header(Row::new(vec![
                "".into(),
                Span::styled("Action", Style::default().italic().blue()),
                Span::styled("Gain", Style::default().italic().blue()),
                Span::styled("EXP Score", Style::default().italic().blue()),
                Span::styled("Win P", Style::default().italic().blue()),
            ]));

        StatefulWidget::render(table, actions_layout[1], buf, &mut self.actions_state);

        Block::bordered()
            .border_set(border::THICK)
            .title(Line::from(" Actions ".bold()).centered())
            .title_bottom(Line::from(vec![
                " Teacher Play ".into(),
                "<SPC> ".blue().bold(),
                " Project Action ".into(),
                "<p> ".blue().bold(),
                " Proceed ".into(),
                "<RET> ".blue().bold(),
                " Quit ".into(),
                "<q> ".blue().bold(),
            ]).right_aligned())
            .render(layout[3], buf);

        Block::bordered()
            .title(" Heuristic Analysis ")
            .title_bottom(Line::from(vec![
                " Show more ".into(),
                "<h> ".blue().bold(),
            ]).right_aligned())
            .render(layout[4], buf);

        Block::bordered()
            .title(" State Analysis ")
            .title_bottom(Line::from(vec![
                " Show more ".into(),
                "<s> ".blue().bold(),
            ]).right_aligned())
            .render(layout[5], buf);

        // Action analysis popup
        if self.show_action_details {
            let block = Block::bordered()
                .border_type(BorderType::Thick)
                .title(" Action Details ");
            let vertical = Layout::vertical([Constraint::Percentage(60)]).flex(Flex::Center);
            let horizontal = Layout::horizontal([Constraint::Percentage(60)]).flex(Flex::Center);
            let [area] = vertical.areas(area);
            let [area] = horizontal.areas(area);
            Clear.render(area, buf);

            let analysis_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(4), Constraint::Length(4), Constraint::Min(10)])
                .split(area);

            let selected_action = self.actions[self.actions_state.selected().unwrap()];
            let mut selected_action_line = Vec::new();
            selected_action_line.push(Line::from(""));
            selected_action_line.push(Line::from(""));

            let selected_display = match selected_action.action_display_choice {
                ActionDisplay::FactoryDisplay(i) => format!("D{}", i),
                ActionDisplay::Center => "Center".to_string()
            };

            let selected_row = match selected_action.pattern_line_choice {
                Some(i) => format!("row {}", i),
                None => "penalty row".to_string()
            };

            selected_action_line.push(Line::from(vec![
                format!("  Move by P{}: ", self.current_player).italic().into(),
                selected_display.into(),
                " ".into(),
                Span::styled("⬛", Style::default().fg(tile_to_color(selected_action.color_choice))),
                " to ".into(),
                selected_row.into()
            ]));

            Paragraph::new(selected_action_line)
                .render(analysis_layout[0], buf);

            let analysis = self.analyses[&selected_action];

            let table = Table::new([
                Row::new(vec!["  Immediate Gain".to_string(), analysis.score_gain.to_string()]),
                Row::new(vec!["  Expected Score".to_string(), if let Some(s) = analysis.expected_score { s.to_string() } else { "NA".to_string() }]),
                Row::new(vec!["  Win Probability".to_string(), if let Some(p) = analysis.win_probability { p.to_string() } else { "NA".to_string() }]),
            ], [
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ])
                .column_spacing(1);

            Widget::render(table, analysis_layout[1], buf);

            let mut rows = vec![];

            for heuristic in &self.heuristics {
                let result = (heuristic.function)(&self.state, self.current_player);
                let applicable = result.is_some();
                let action_match = if let Some(heuristic_action) = result {
                    heuristic_action == selected_action
                } else { false };

                rows.push(Row::new(vec![format!("  {}", heuristic.name), applicable.to_string(), action_match.to_string()]));
            }

            let table = Table::new(rows, [
                Constraint::Percentage(60),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
                .column_spacing(1)
                .header(Row::new(vec!["  Heuristic", "Applicable", "Match"]));

            Widget::render(table, analysis_layout[2], buf);

            block.render(area, buf);
        }
    }
}
