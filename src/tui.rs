use std::collections::HashMap;
use std::ops::BitOrAssign;

use crate::games::azul::ActionDisplay;
use crate::games::GameState;

use super::azul::{self, Tile, WALL_COLORS};
use ratatui::layout::{Constraint, Direction, Flex, Layout};
use ratatui::style::{self, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{BorderType, Borders, Clear, HighlightSpacing, List, ListState, Row, StatefulWidget, Table};

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
    pub expected_score: f32,
    pub win_probability: f32,
}

#[derive(Clone)]
pub struct InteractiveApp {
    pub state: azul::State,
    pub current_player: usize,
    pub ply: usize,
    pub ply_round: usize,
    pub last_move: Option<Move>,
    pub actions: Vec<azul::Action>,
    pub actions_state: ListState,
    pub analyses: HashMap<azul::Action, ActionAnalysis>,
    pub show_action_details: bool,
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

// Format action for the main window selector
fn action_span(action: &azul::Action, action_idx: usize) -> Line {
    let display = match action.action_display_choice {
        ActionDisplay::FactoryDisplay(i) => format!("D{}", i),
        ActionDisplay::Center => "Center".to_string()
    };

    let row = match action.pattern_line_choice {
        Some(i) => format!("row {}", i),
        None => "penalty row".to_string()
    };

    Line::from(vec![
        Span::styled(format!(" {:>3}. ", action_idx), Style::default()),
        "Take ".into(),
        Span::styled("⬛", Style::default().fg(tile_to_color(action.color_choice))),
        " from ".into(),
        display.into(),
        ", put in ".into(),
        row.into()
    ])
}

impl Widget for InteractiveApp {
    fn render(mut self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(7),
                Constraint::Length(12),
                Constraint::Length(22),
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

        self.state.center.render(display_layout[1], buf);

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
            .constraints(vec![Constraint::Max(4), Constraint::Max(17)])
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
                    "Take ".into(),
                    Span::styled("⬛", Style::default().fg(tile_to_color(mov.action.color_choice))),
                    " from ".into(),
                    display.into(),
                    ", put in ".into(),
                    row.into()
                ]));
            },
            None => {
                last_move_lines.push("        Last Move: NA".italic().into())
            }
        }

        Paragraph::new(last_move_lines)
            .render(actions_layout[0], buf);

        let items = List::new(self.actions.iter().enumerate().map(|(idx, action)| action_span(action, idx)))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol(" →")
            .highlight_spacing(HighlightSpacing::Always)
            .repeat_highlight_symbol(true);

        StatefulWidget::render(items, actions_layout[1], buf, &mut self.actions_state);

        let block = Block::bordered()
            .title(Line::from(" Actions ".bold()).centered())
            .title_bottom(Line::from(vec![
                " Teacher Play ".into(),
                "<SPC> ".blue().bold(),
                " Analyze Action ".into(),
                "<a> ".blue().bold(),
                " Proceed ".into(),
                "<RET> ".blue().bold(),
                " Quit ".into(),
                "<q> ".blue().bold(),
            ]).right_aligned());

        block.render(layout[3], buf);

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
                "Take ".into(),
                Span::styled("⬛", Style::default().fg(tile_to_color(selected_action.color_choice))),
                " from ".into(),
                selected_display.into(),
                ", put in ".into(),
                selected_row.into()
            ]));

            Paragraph::new(selected_action_line)
                .render(area, buf);

            let analysis = self.analyses[&selected_action];

            let table = Table::new([
                Row::new(vec!["", ""]),
                Row::new(vec!["", ""]),
                Row::new(vec!["", ""]),
                Row::new(vec!["", ""]),
                Row::new(vec!["  Immediate Gain".to_string(), analysis.score_gain.to_string()]),
                Row::new(vec!["  Expected Score".to_string(), analysis.expected_score.to_string()]),
                Row::new(vec!["  Win Probability".to_string(), analysis.win_probability.to_string()]),
            ], [
                Constraint::Percentage(80),
                Constraint::Percentage(20),
            ])
                .column_spacing(1);

            Widget::render(table, area, buf);
            block.render(area, buf);
        }
    }
}
