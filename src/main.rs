use games::Validate;

mod games;

fn main() {
    env_logger::init();

    let n_players: usize = 3;
    let mut state = games::azul::State::new(n_players);

    if let Err(err) = state.validate() {
        println!("{}", err);
        return;
    }

    loop {
        log::debug!("Round: {}", state.rounds);
        let mut current_player = match games::azul::first_player(&state) {
            Some(one) => {
                state.players[one].starting_marker = false;
                one
            },
            None => 0,
        };

        log::debug!("Starting player: {}", current_player);

        games::azul::refill_tiles(&mut state);
        loop {
            // If tiles are over, round stops
            if state.has_no_tiles() {
                state.rounds += 1;
                break;
            }
            log::info!("Picking from {} actions for P{}", games::azul::list_valid_actions(&state, current_player).len(), current_player);
            games::azul::play_greedy(&mut state, current_player);
            current_player += 1;
            current_player %= n_players;
        }

        for i in 0..n_players {
            games::azul::tile_wall_and_score(&mut state, i);
            log::debug!("Score P{}: {}", i, state.players[i].score);
        }

        if games::azul::is_game_over(&state) {
            break;
        }
    }

    for i in 0..n_players {
        log::info!("Final score P{}: {}", i, state.players[i].score);
    }

    log::info!("Winner is P{}", games::azul::winner(&state));
}
