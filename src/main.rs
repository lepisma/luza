use std::fs::File;
use std::io::BufWriter;
use std::{collections::HashMap, path::PathBuf};
use std::sync::{Arc, Mutex};
use color_eyre::owo_colors::OwoColorize;
use games::azul::{play_greedy, play_mcts, play_random, take_action, Tile, WALL_COLORS};
use log::debug;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{self, Style};
use ratatui::text::Span;
use ratatui::widgets::{BorderType, Borders};
use rayon::iter::ParallelIterator;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal,
};
use games::{azul, Validate, GameState};
use rayon::iter::IntoParallelIterator;
use clap::{Parser, Subcommand};

mod games;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Simulate {
        #[arg(short)]
        log_file: PathBuf,
        game: String,
    },
    IIL {
        game: String,
    },
}

// One ply in the game log, the string representations here are serialized data
// points and not only vectors
#[derive(Debug, Clone, serde::Serialize)]
struct PlayLogPly {
    game_id: usize,
    round_id: i32,
    ply_id: i32,
    player_id: i32,
    action: String,
    state: String,
    score: i32,
    applicable_partials: Vec<String>,
    matching_partials: Vec<String>
}

trait Feature {
    fn display(&self) -> String;
}

type FeatureFn = fn(&azul::State) -> Box<dyn Feature>;
type SetupStateFn = fn(&azul::State) -> bool;
type PlayFn = fn(&azul::State, usize) -> azul::Action;
type PartialPlayFn = fn(&azul::State, usize) -> Option<azul::Action>;

type PlayLog = Vec<PlayLogPly>;

fn write_play_log(play_log: &PlayLog, file: &PathBuf) {
    let file = File::create(file).unwrap();
    let mut writer = BufWriter::new(file);
    for item in play_log {
        jsonl::write(&mut writer, item).unwrap();
    }
}

fn report(game_log: Vec<usize>, n_players: usize) {
    let mut win_counts: HashMap<usize, usize> = HashMap::from_iter((0..n_players).map(|i| (i, 0)));
    let total_games = game_log.len();

    for i in game_log {
        let new_score = win_counts[&i] + 1;
        if let Some(current) = win_counts.get_mut(&i) {
            *current = new_score;
        }
    }

    for i in 0..n_players {
        println!("Win Count for P{}: {}/{}, ratio: {}", i, win_counts[&i], total_games, (win_counts[&i] as f64 / total_games as f64));
    }
}

fn simulate(_game: &str, log_file: &PathBuf, n_sims: usize) {
    let players: Vec<PlayFn> = [
        azul::play_greedy,
        azul::play_mcts,
    ].to_vec();
    let n_players = players.len();
    // MCTS has been consistently doing better than greedy in our trials
    let best_player_idx = 1;

    // Partial functions that need to be put against the best player
    let partials: Vec<(String, PartialPlayFn)> = [
        ("greedy".to_string(), azul::play_partial_greedy as PartialPlayFn),
        ("random".to_string(), azul::play_partial_random as PartialPlayFn),
    ].to_vec();

    log::info!("Running {} simulations for {} players,", n_sims, n_players);

    let play_log: Arc<Mutex<PlayLog>> = Arc::new(Mutex::new(Vec::new()));

    let game_log: Vec<usize> = (0..n_sims).into_par_iter().map(|game_idx| {
        let mut state = azul::State::new(n_players);

        play_log.lock().unwrap().push(PlayLogPly {
            game_id: game_idx,
            round_id: -1,
            ply_id: -1,
            player_id: -1,
            action: "init".to_string(),
            state: serde_json::to_string(&state).unwrap(),
            score: 0,
            applicable_partials: Vec::new(),
            matching_partials: Vec::new(),
        });

        if let Err(err) = state.validate() {
            println!("{}", err);
            return usize::MAX;
        }

        let mut ply_id: i32 = 0;
        let mut round_id: i32 = 0;
        loop {
            log::debug!("Round: {}", state.rounds);
            let mut current_player = match azul::first_player(&state) {
                Some(one) => {
                    state.players[one].starting_marker = false;
                    one
                },
                None => 0,
            };

            log::debug!("Starting player: {}", current_player);

            azul::refill_tiles(&mut state);
            play_log.lock().unwrap().push(PlayLogPly {
                game_id: game_idx,
                round_id: -1,
                ply_id: -1,
                player_id: -1,
                action: "reset-round".to_string(),
                state: serde_json::to_string(&state).unwrap(),
                score: 0,
                applicable_partials: Vec::new(),
                matching_partials: Vec::new(),
            });

            loop {
                // If tiles are over, round stops
                if state.is_round_over() {
                    state.rounds += 1;
                    break;
                }
                let action = players[current_player](&state, current_player);

                // Partial fn matching
                let mut applicable_partials: Vec<String> = Vec::new();
                let mut matching_partials: Vec<String> = Vec::new();
                if current_player == best_player_idx {
                    for (p_name, p_fn) in partials.clone() {
                        if let Some(p_action) = p_fn(&state, current_player) {
                            applicable_partials.push(p_name.clone());
                            if p_action == action {
                                matching_partials.push(p_name);
                            }
                        }
                    }
                }
                azul::take_action(&mut state, current_player, action);

                let mut state_clone = state.clone();
                azul::score_round(&mut state_clone, current_player);

                play_log.lock().unwrap().push(PlayLogPly {
                    game_id: game_idx,
                    round_id,
                    ply_id,
                    player_id: current_player as i32,
                    action: serde_json::to_string(&action).unwrap(),
                    state: serde_json::to_string(&state).unwrap(),
                    score: state_clone.players[current_player].score,
                    applicable_partials,
                    matching_partials,
                });

                current_player += 1;
                current_player %= n_players;
                ply_id += 1;
            }

            for i in 0..n_players {
                azul::score_round(&mut state, i);
                log::debug!("Score P{}: {}", i, state.players[i].score);
            }
            round_id += 1;

            if state.is_game_over() {
                break;
            }
        }

        for i in 0..n_players {
            log::info!("Final score P{}: {}", i, state.players[i].score);
        }
        log::info!("Winner is P{}", azul::winner(&state));

        azul::winner(&state)
    }).collect();

    report(game_log, n_players);
    write_play_log(&play_log.lock().unwrap().to_vec(), log_file);
}

fn iil(_game: &str) {
    // Inverse Imitation Learning for Azul
    let teacher: PlayFn = azul::play_mcts;
    let features: Vec<FeatureFn> = Vec::new();
    let setup_states: Vec<SetupStateFn> = Vec::new();
    let action_heuristics: Vec<PartialPlayFn> = Vec::new();

    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let _result = iil_run(terminal);
    ratatui::restore();
}

#[derive(Clone)]
struct IILApp {
    state: azul::State,
    current_player: usize,
    ply: usize,
    ply_round: usize,
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

fn tile_to_color(tile: Tile) -> style::Color {
    match tile {
        Tile::Black => style::Color::Black,
        Tile::Blue => style::Color::Blue,
        Tile::Red => style::Color::Red,
        Tile::White => style::Color::White,
        Tile::Yellow => style::Color::Yellow,
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

impl Widget for IILApp {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(7),
                Constraint::Length(12),
                Constraint::Length(6),
            ])
            .split(area);

        let title = Line::from(" Game Info ".bold());
        let block = Block::bordered()
            .title(title.centered())
            .border_set(border::THICK);

        let header_text = Text::from(vec![Line::from(vec![
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

            Block::bordered().render(factory_layout[i], buf);
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

        let block = Block::bordered()
            .title(Line::from(" Actions ".bold()).centered())
            .title_bottom(Line::from(vec![
                " Quit ".into(),
                "<qq> ".blue().bold(),
            ]).right_aligned());

        Paragraph::new(Text::from(""))
            .block(block)
            .render(layout[3], buf);
    }
}

fn iil_run(mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
    let n_players = 3;

    let mut app = IILApp{
        state: azul::State::new(n_players),
        current_player: 0,
        ply: 0,
        ply_round: 0
    };

    loop {
        app.current_player = match azul::first_player(&app.state) {
            Some(one) => {
                app.state.players[one].starting_marker = false;
                one
            },
            None => 0,
        };

        azul::refill_tiles(&mut app.state);
        terminal.draw(|frame| {
            frame.render_widget(app.clone(), frame.area());
        })?;

        loop {
            terminal.draw(|frame| {
                frame.render_widget(app.clone(), frame.area());
            })?;

            if app.state.is_round_over() {
                app.state.rounds += 1;
                app.ply_round = 0;
                break;
            }

            match event::read()? {
                Event::Key(key_event) => {
                    match key_event.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Enter => {
                            let action = play_greedy(&app.state, app.current_player);
                            take_action(&mut app.state, app.current_player, action);

                            app.current_player += 1;
                            app.current_player %= n_players;
                            app.ply += 1;
                            app.ply_round += 1;
                        },
                        _ => {}
                    }
                },
                _ => {}
            };
        }
        for i in 0..n_players {
            azul::score_round(&mut app.state, i);
        }

        if app.state.is_game_over() {
            break Ok(())
        }

        match event::read()? {
            Event::Key(key_event) => {
                match key_event.code {
                    KeyCode::Char('q') => break Ok(()),
                    _ => {}
                }
            },
            _ => { }
        }
    }
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    match args.commands {
        Commands::Simulate { log_file, game } => simulate(&game, &log_file, 10),
        Commands::IIL { game } => iil(&game),
    }
}
