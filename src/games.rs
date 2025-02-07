use anyhow::Result;

pub mod azul;

pub trait Validate {
    fn validate(&self) -> Result<()>;
}

pub trait Representable {
    fn represent(&self) -> Vec<f64>;
}

// A game state for sequential games with n player. This is played in many
// rounds where players take plys in sequence.
pub trait GameState {
    fn new(n_players: usize) -> Self;

    // Tell if a round is over. A round might be over but the game might not be.
    fn is_round_over(&self) -> bool;

    // Tell if the game is over. Also see `is_round_over`.
    fn is_game_over(&self) -> bool;
}
