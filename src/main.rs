use ratatui::widgets::ListState;
use tui::{ActionAnalysis, InteractiveApp};
use std::fs::File;
use std::io::BufWriter;
use std::{collections::HashMap, path::PathBuf};
use std::sync::{Arc, Mutex};
use rayon::iter::ParallelIterator;

use crossterm::event::{self, Event, KeyCode};
use games::{azul, Validate, GameState};
use rayon::iter::IntoParallelIterator;
use clap::{Parser, Subcommand};

mod games;
mod tui;

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
    Interactive {
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

fn run_interactive(_game: &str) {
    let teacher: PlayFn = azul::play_mcts;
    let _action_heuristics: Vec<PartialPlayFn> = Vec::new();

    color_eyre::install().unwrap();
    let mut terminal = ratatui::init();
    let n_players = 3;

    let mut app = InteractiveApp{
        state: azul::State::new(n_players),
        current_player: 0,
        ply: 0,
        ply_round: 0,
        actions: Vec::new(),
        actions_state: ListState::default(),
        last_move: None,
        analysis: None,
    };

    let mut user_exit = false;

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
        }).unwrap();

        loop {
            terminal.draw(|frame| {
                frame.render_widget(app.clone(), frame.area());
            }).unwrap();

            if app.state.is_round_over() {
                app.state.rounds += 1;
                app.ply_round = 0;
                break;
            }

            app.actions = azul::list_valid_actions(&app.state, app.current_player);
            app.actions.sort_by_key(|a| -azul::calculate_reward(&app.state, app.current_player, a.clone()));

            terminal.draw(|frame| {
                frame.render_widget(app.clone(), frame.area());
            }).unwrap();

            match event::read().unwrap() {
                Event::Key(key_event) => {
                    if let Some(_) = app.analysis {
                        // When the popup is open, only exiting is allowed
                        match key_event.code {
                            KeyCode::Char('q') => {
                                app.analysis = None;
                            },
                            _ => {}
                        }
                    } else {
                        match key_event.code {
                            KeyCode::Char('q') => {
                                user_exit = true;
                                break;
                            },
                            KeyCode::Char(' ') => {
                                let action = teacher(&app.state, app.current_player);
                                azul::take_action(&mut app.state, app.current_player, action);

                                app.last_move = Some(tui::Move {
                                    player: app.current_player,
                                    action: action.clone()
                                });

                                app.actions_state.select_first();
                                app.current_player += 1;
                                app.current_player %= n_players;
                                app.ply += 1;
                                app.ply_round += 1;
                            },
                            KeyCode::Enter => {
                                match app.actions_state.selected() {
                                    Some(action_idx) => {
                                        let action = app.actions[action_idx];
                                        azul::take_action(&mut app.state, app.current_player, action);

                                        app.last_move = Some(tui::Move {
                                            player: app.current_player,
                                            action: action.clone()
                                        });

                                        app.actions_state.select_first();
                                        app.current_player += 1;
                                        app.current_player %= n_players;
                                        app.ply += 1;
                                        app.ply_round += 1;
                                    },
                                    None => {}
                                };
                            },
                            KeyCode::Down => {
                                if let Some(action_idx) = app.actions_state.selected() {
                                    if action_idx < app.actions.len() - 1 {
                                        app.actions_state.select_next();
                                    }
                                } else {
                                    app.actions_state.select_first();
                                }
                            },
                            KeyCode::Up => {
                                if let Some(_) = app.actions_state.selected() {
                                    app.actions_state.select_previous();
                                } else {
                                    app.actions_state.select_first();
                                }
                            },
                            KeyCode::Char('a') => {
                                if let Some(_) = app.actions_state.selected() {
                                    app.analysis = Some(ActionAnalysis {
                                        score_gain: 0,
                                        expected_score: 0,
                                        win_probability: 0.0,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                },
                _ => {}
            };

            terminal.draw(|frame| {
                frame.render_widget(app.clone(), frame.area());
            }).unwrap();
        }
        for i in 0..n_players {
            azul::score_round(&mut app.state, i);
        }

        if app.state.is_game_over() || user_exit {
            break;
        }
    }

    // Listen to Q unless user has showed intention to quit already
    if !user_exit {
        terminal.draw(|frame| {
            frame.render_widget(app.clone(), frame.area());
        }).unwrap();

        loop {
            match event::read().unwrap() {
                Event::Key(key_event) => {
                    match key_event.code {
                        KeyCode::Char('q') => { break; },
                        _ => {}
                    }
                },
                _ => {}
            };
        }
    }

    ratatui::restore();
}

fn main() {
    env_logger::init();
    let args = Args::parse();

    match args.commands {
        Commands::Simulate { log_file, game } => simulate(&game, &log_file, 100),
        Commands::Interactive { game } => run_interactive(&game),
    }
}
