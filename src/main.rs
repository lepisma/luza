use std::collections::HashMap;

use games::{azul, Validate, GameState};

mod games;

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

fn main() {
    env_logger::init();

    // Number of simulations to run for reporting
    let n_games: usize = 50;

    let players = [
        azul::play_greedy,
        azul::play_mcts,
    ];
    let n_players = players.len();

    // Pair of winning player id and score
    let mut game_log: Vec<usize> = Vec::with_capacity(n_games);
    log::info!("Running {} simulations for {} players,", n_games, n_players);

    for _ in 0..n_games {
        let mut state = azul::State::new(n_players);

        if let Err(err) = state.validate() {
            println!("{}", err);
            return;
        }

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
            loop {
                // If tiles are over, round stops
                if state.is_round_over() {
                    state.rounds += 1;
                    break;
                }
                log::debug!("Picking from {} actions for P{}", azul::list_valid_actions(&state, current_player).len(), current_player);
                players[current_player](&mut state, current_player);
                current_player += 1;
                current_player %= n_players;
            }

            for i in 0..n_players {
                azul::tile_wall_and_score(&mut state, i);
                log::debug!("Score P{}: {}", i, state.players[i].score);
            }

            if state.is_game_over() {
                break;
            }
        }

        for i in 0..n_players {
            log::info!("Final score P{}: {}", i, state.players[i].score);
        }
        log::info!("Winner is P{}", azul::winner(&state));
        game_log.push(azul::winner(&state));
    }

    report(game_log, n_players);
}
