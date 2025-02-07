use super::{Representable, Validate, GameState};
use std::{collections::HashMap, vec};
use anyhow::{anyhow, Result};
use rand::{distr::{weighted::WeightedIndex, Distribution}, seq::IndexedRandom, seq::IteratorRandom, Rng};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Tile {
    Black, Blue, Red, White, Yellow,
}

const COLORS: [Tile; 5] = [Tile::Black, Tile::Blue, Tile::Red, Tile::White, Tile::Yellow];
const FLOOR_PENALTIES: [usize; 7] = [1, 1, 2, 2, 2, 3, 3];
const WALL_COLORS: [[Tile; 5]; 5] = [
    [Tile::Blue, Tile::Yellow, Tile::Red, Tile::Black, Tile::White],
    [Tile::White, Tile::Blue, Tile::Yellow, Tile::Red, Tile::Black],
    [Tile::Black, Tile::White, Tile::Blue, Tile::Yellow, Tile::Red],
    [Tile::Red, Tile::Black, Tile::White, Tile::Blue, Tile::Yellow],
    [Tile::Yellow, Tile::Red, Tile::Black, Tile::White, Tile::Blue],
];

type FactoryDisplayState = HashMap<Tile, usize>;

#[derive(Debug, Clone)]
struct CenterState {
    tiles: HashMap<Tile, usize>,
    starting_marker: bool,
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub score: i32,
    wall: [[bool; 5]; 5],
    pattern_lines: [(Option<Tile>, usize); 5],
    floor_line: usize,
    pub starting_marker: bool,
}

#[derive(Debug, Clone)]
pub struct State {
    factory_displays: Vec<FactoryDisplayState>,
    center: CenterState,
    pub players: Vec<PlayerState>,
    pub rounds: usize,
}

// Action that tells which tile stash is picked by a player
#[derive(Clone, Copy, Debug)]
enum ActionDisplay {
    FactoryDisplay(usize),
    Center
}

#[derive(Clone, Debug, Copy)]
pub struct Action {
    action_display_choice: ActionDisplay,
    color_choice: Tile,
    pattern_line_choice: Option<usize>,
}

impl<T: Representable> Representable for Vec<T> {
    fn represent(&self) -> Vec<f64> {
        let mut vec = Vec::new();
        for item in self {
            vec.extend(item.represent());
        }
        vec
    }
}

impl Representable for Tile {
    fn represent(&self) -> Vec<f64> {
        let mut vec = vec![0.0; 5];
        let idx = COLORS.iter().position(|c| c == self).unwrap();
        vec[idx] = 1.0;
        vec
    }
}

impl Representable for FactoryDisplayState {
    fn represent(&self) -> Vec<f64> {
        let mut vec = Vec::with_capacity(5 * (5 + 1));

        for i in 0..5 {
            let color = COLORS[i];
            vec.extend(color.represent().iter());
            vec.push(self[&color] as f64)
        }

        vec
    }
}

impl Representable for CenterState {
    fn represent(&self) -> Vec<f64> {
        let mut vec = Vec::with_capacity(5 * (5 + 1) + 1);

        for i in 0..5 {
            let color = COLORS[i];
            vec.extend(color.represent().iter());
            vec.push(self.tiles[&color] as f64);
        }

        vec.push(self.starting_marker as i32 as f64);
        vec
    }
}

impl Representable for PlayerState {
    fn represent(&self) -> Vec<f64> {
        // The size is based on flat representation of all items in the state
        let mut vec = Vec::with_capacity(1 + (5 * 5) + 5 * (5 + 1) + 1 + 1);

        vec.push(self.score as f64);
        for i in 0..5 {
            for j in 0..5 {
                vec.push(self.wall[i][j] as usize as f64);
            }
        }

        for i in 0..5 {
            match self.pattern_lines[i] {
                (None, count) => {
                    vec.extend(vec![0.0; 5].iter());
                    vec.push(count as f64);
                },
                (Some(tile), count) => {
                    vec.extend(tile.represent().iter());
                    vec.push(count as f64);
                },
            }
        }

        vec.push(self.floor_line as f64);
        vec.push(self.starting_marker as i32 as f64);
        vec
    }
}

impl Representable for State {
    fn represent(&self) -> Vec<f64> {
        let mut vec = Vec::new();

        vec.extend(self.factory_displays.represent());
        vec.extend(self.center.represent());
        vec.extend(self.players.represent());
        vec.push(self.rounds as f64);
        vec
    }
}

impl Representable for ActionDisplay {
    fn represent(&self) -> Vec<f64> {
        let mut vec: Vec<f64> = Vec::with_capacity(1);
        // Note that this will make the representation very tied to the number
        // of players
        match self {
            Self::FactoryDisplay(i) => vec.push(*i as f64),
            Self::Center => vec.push(-1.0)
        }

        vec
    }
}

impl Representable for Action {
    fn represent(&self) -> Vec<f64> {
        let mut vec = Vec::new();

        vec.extend(self.action_display_choice.represent());
        vec.extend(self.color_choice.represent());
        match self.pattern_line_choice {
            None => vec.push(5.0),
            Some(line_idx) => vec.push(line_idx as f64),
        }

        vec
    }
}

impl CenterState {
    fn new() -> Self {
        Self {
            tiles: HashMap::from([
                (Tile::Black, 0),
                (Tile::Blue, 0),
                (Tile::Red, 0),
                (Tile::White, 0),
                (Tile::Yellow, 0),
            ]),
            starting_marker: true,
        }
    }

    fn has_no_tiles(&self) -> bool {
        self.tiles.values().sum::<usize>() == 0
    }
}

impl PlayerState {
    fn new() -> Self {
        Self {
            score: 0,
            wall: [[false; 5]; 5],
            pattern_lines: [(None, 0); 5],
            floor_line: 0,
            starting_marker: false,
        }
    }

    // Tell if a player has completed at last one row. If this happen, the game gets over after the current round.
    fn has_completed_row(&self) -> bool {
        for i in 0..5 {
            if self.wall[i].iter().all(|&x| x) {
                return true;
            }
        }

        false
    }
}

fn build_empty_display() -> HashMap<Tile, usize> {
    HashMap::from([
        (Tile::Black, 0),
        (Tile::Blue, 0),
        (Tile::Red, 0),
        (Tile::White, 0),
        (Tile::Yellow, 0),
    ])
}

impl GameState for State {
    // Create new game with empty displays
    fn new(n_players: usize) -> Self {
        let n_displays = (n_players * 2) + 1;
        let mut factory_displays: Vec<FactoryDisplayState> = Vec::with_capacity(n_displays);
        for _i in 0..n_displays {
            factory_displays.push(build_empty_display());
        }

        State {
            factory_displays,
            center: CenterState::new(),
            players: vec![PlayerState::new(); n_players],
            rounds: 0,
        }
    }

    fn is_round_over(&self) -> bool {
        self.center.has_no_tiles() && self.factory_displays
            .iter()
            .map(|d| has_no_tiles(d.clone()))
            .any(|x| x)
    }

    fn is_game_over(&self) -> bool {
        self.is_round_over() && self.players.iter().any(|p| p.has_completed_row())
    }
}

// Refill tiles in factory_displays, resetting center
pub fn refill_tiles(state: &mut State) {
    let mut rng = rand::rng();

    for display in &mut state.factory_displays {
        for _i in 0..4 {
            let tile = COLORS.choose(&mut rng).unwrap();
            if let Some(count) = display.get_mut(&tile) {
                *count += 1;
            }
        }
    }

    state.center = CenterState::new();
}

impl Validate for State {
    fn validate(&self) -> Result<()> {
        let n_players = self.players.len();
        if n_players < 2 || n_players > 4 {
            return Err(anyhow!("Number of players ({}) outside the bound [2, 4]", n_players));
        }

        Ok(())
    }
}

fn has_no_tiles(display: FactoryDisplayState) -> bool {
    display.values().sum::<usize>() == 0
}

// Mutate the game state and take out given color tiles based on the action
fn take_out_tiles(state: &mut State, action: ActionDisplay, color: Tile) -> Vec<Tile> {
    let count = match action {
        ActionDisplay::Center => {
            let count = state.center.tiles[&color];
            if let Some(v) = state.center.tiles.get_mut(&color) {
                *v = 0
            }
            count
        },
        ActionDisplay::FactoryDisplay(i) => {
            let mut count: usize = 0;
            for c in COLORS {
                let c_count = state.factory_displays[i][&c];
                if color == c {
                    count = c_count;
                } else {
                    if let Some(v) = state.center.tiles.get_mut(&c) {
                        *v += c_count;
                    }
                }
                if let Some(v) = state.factory_displays[i].get_mut(&c) {
                    *v = 0
                }
            }

            count
        },
    };

    vec![color; count]
}

fn wall_row_has_color(wall: &[[bool; 5]; 5], row_idx: usize, color: Tile) -> bool {
    let row_colors = WALL_COLORS[row_idx];
    let wall_row = wall[row_idx];

    let idx = row_colors.iter().position(|&c| c == color).unwrap();
    wall_row[idx]
}

fn find_empty_lines(state: &State, color: Tile, player_idx: usize) -> Vec<usize> {
    // A line is not available if it has a color other than given, if it has no
    // space, or its wall row has the same color filled.
    let mut empty_line_ids: Vec<usize> = Vec::new();

    for i in 0..5 {
        let line_size = i + 1;
        let line = state.players[player_idx].pattern_lines[i];
        let wall = state.players[player_idx].wall;

        match line.0 {
            None => { if !wall_row_has_color(&wall, i, color) { empty_line_ids.push(i) } },
            Some(tile) => { if tile == color && line_size > line.1 { empty_line_ids.push(i) } }
        }
    }

    empty_line_ids
}

// Put tiles in the pattern and floor lines
fn stage_tiles(state: &mut State, player_idx: usize, line: Option<usize>, color: Tile, count: usize) {
    match line {
        None => {
            state.players[player_idx].floor_line += count;
        },
        Some(idx) => {
            let line_size = idx + 1;
            let space = line_size - state.players[player_idx].pattern_lines[idx].1;

            if space < count {
                state.players[player_idx].pattern_lines[idx] = (Some(color), space);
                // Penalize for the leftovers
                state.players[player_idx].floor_line += count - space;
            } else {
                state.players[player_idx].pattern_lines[idx] = (Some(color), count);
            }
        }
    }

    // Clamp floor line
    state.players[player_idx].floor_line = std::cmp::min(state.players[player_idx].floor_line, 7);
}

// List all valid lines that can be considered for given color and player. None
// means choosing floor line.
fn list_valid_lines(state: &State, player_idx: usize, color: Tile) -> Vec<Option<usize>> {
    let empty_lines = find_empty_lines(state, color, player_idx);
    let mut lines: Vec<Option<usize>> = empty_lines.iter().map(|&i| Some(i)).collect();
    lines.push(None);
    lines
}

// List all valid actions available to the player
pub fn list_valid_actions(state: &State, player_idx: usize) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();

    for display_idx in 0..state.factory_displays.len() {
        if state.factory_displays[display_idx].is_empty() {
            continue;
        }

        for color in COLORS {
            if state.factory_displays[display_idx][&color] > 0 {
                for line in list_valid_lines(state, player_idx, color) {
                    actions.push(Action {
                        action_display_choice: ActionDisplay::FactoryDisplay(display_idx),
                        color_choice: color,
                        pattern_line_choice: line,
                    })
                }
            }
        }
    }

    if !state.center.has_no_tiles() {
        for color in COLORS {
            if state.center.tiles[&color] > 0 {
                for line in list_valid_lines(state, player_idx, color) {
                    actions.push(Action {
                        action_display_choice: ActionDisplay::Center,
                        color_choice: color,
                        pattern_line_choice: line,
                    })
                }
            }
        }
    }

    actions
}

fn count_continuous(array: &[bool; 5], anchor: usize) -> usize {
    let mut count = 0;
    let mut curr: i32;

    for i in 1..5 {
        curr = anchor as i32 - i;
        if curr < 0 {
            break;
        }
        if array[curr as usize] {
            count += 1;
        } else {
            break;
        }
    }

    for i in 1..5 {
        curr = anchor as i32 + i;
        if curr > 4 {
            break;
        }
        if array[curr as usize] {
            count += 1;
        } else {
            break;
        }
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_continuous() {
        assert_eq!(count_continuous(&[false, true, true, false, false], 0), 2);
        assert_eq!(count_continuous(&[false, true, false, false, false], 2), 1);
        assert_eq!(count_continuous(&[false, false, false, false, false], 2), 0);
        assert_eq!(count_continuous(&[false, true, true, true, false], 4), 3);
        assert_eq!(count_continuous(&[true, true, true, false, false], 3), 3);
    }
}

fn score_placement(wall: &[[bool; 5]; 5], row_idx: usize, color: Tile) -> i32 {
    let mut score: i32 = 0;

    let col_idx = WALL_COLORS[row_idx].iter().position(|&x| x == color).unwrap();
    let col = (0..5).map(|i| wall[row_idx][i]).collect::<Vec<bool>>();

    // Basic adjacency checks
    let col_continuous = count_continuous(&col.clone().try_into().expect("Failed to convert column in a bool array"), col_idx);
    let row_continuous = count_continuous(&wall[row_idx], row_idx);
    score += std::cmp::max((col_continuous + row_continuous) as i32, 1);

    // Check if col gets completed
    let col_completed: bool = col
        .iter()
        .enumerate()
        .map(|(i, &x)| if i == row_idx { true } else { x })
        .all(|x| x);

    if col_completed {
        score += 7
    }

    // Check if row gets completed
    let row_completed: bool = wall[row_idx]
        .iter()
        .enumerate()
        .map(|(i, &x)| if i == col_idx { true } else { x })
        .all(|x| x);

    if row_completed {
        score += 2
    }

    // Check if color gets completed
    let mut color_coverage: usize = 0;

    for i in 0..5 {
        for j in 0..5 {
            if i == row_idx && j == col_idx {
                color_coverage += 1;
            } else {
                if WALL_COLORS[i][j] == color && wall[i][j] {
                    color_coverage += 1;
                }
            }
        }
    }

    if color_coverage == 5 {
        score += 10
    }

    score
}

pub fn tile_wall_and_score(state: &mut State, player_idx: usize) {
    let mut accumulator: i32 = 0;

    let mut tiling_points = 0;
    for i in 0..5 {
        let line_size = i + 1;
        if state.players[player_idx].pattern_lines[i].1 == line_size {
            let color = state.players[player_idx].pattern_lines[i].0.unwrap();
            tiling_points += score_placement(&state.players[player_idx].wall, i, color);
            let col_idx = WALL_COLORS[i].iter().position(|&x| x == color).unwrap();
            state.players[player_idx].wall[i][col_idx] = true;
            state.players[player_idx].pattern_lines[i] = (None, 0);
        }
    }
    accumulator += tiling_points;

    // Take penalties, if any
    let penalties = FLOOR_PENALTIES.iter().take(state.players[player_idx].floor_line).sum::<usize>() as i32;
    accumulator -= penalties;

    state.players[player_idx].score += accumulator;
    state.players[player_idx].score = std::cmp::max(state.players[player_idx].score, 0);

    state.players[player_idx].floor_line = 0;
}

// Tell if one of the players has starting marker
pub fn first_player(state: &State) -> Option<usize> {
    for i in 0..state.players.len() {
        if state.players[i].starting_marker {
            return Some(i)
        }
    }

    None
}

pub fn winner(state: &State) -> usize {
    state.players
        .iter()
        .enumerate()
        .max_by_key(|(_i, p)| p.score)
        .unwrap()
        .0
}

// Apply action to the state for the given player. Assume that the action is
// valid and won't cause any issue. The action generator has to ensure this.
fn take_action(state: &mut State, player_idx: usize, action: Action) {
    let tiles = take_out_tiles(state, action.action_display_choice, action.color_choice);

    // In case the action involves picking from center, take the starting marker
    // if not already taken
    if let ActionDisplay::Center = action.action_display_choice {
        if state.center.starting_marker {
            state.players[player_idx].starting_marker = true;
            state.center.starting_marker = false
        }
    }

    stage_tiles(state, player_idx, action.pattern_line_choice, action.color_choice, tiles.len());
}

// Return reward of taking action for given player with given game state. The
// current score is calculated so you don't have to worry about ply count etc.
fn calculate_reward(state: &State, player_idx: usize, action: Action) -> i32 {
    let mut state_clone_a = state.clone();
    let mut state_clone_b = state.clone();

    // This is needed since if this is not the first ply of the player in given
    // round, they already might have more score than what's noted in state at
    // the moment.
    tile_wall_and_score(&mut state_clone_a, player_idx);

    take_action(&mut state_clone_b, player_idx, action);
    tile_wall_and_score(&mut state_clone_b, player_idx);

    // Calculate what gain will we have just from this action
    state_clone_b.players[player_idx].score - state_clone_a.players[player_idx].score
}

// Choose a random action from the list of valid actions available to the
// player.
pub fn play_random(state: &mut State, player_idx: usize) {
    let mut rng = rand::rng();
    let action = list_valid_actions(state, player_idx).choose(&mut rng).unwrap().clone();
    take_action(state, player_idx, action);
}

// See all possible actions and choose the one that has highest immediate reward
// for the player.
pub fn play_greedy(state: &mut State, player_idx: usize) {
    let action = list_valid_actions(state, player_idx).into_iter().max_by_key(|a| calculate_reward(state, player_idx, a.clone())).unwrap().clone();
    log::debug!("Action: {:?}", action);
    take_action(state, player_idx, action);
}

fn rewards_dist(rewards: Vec<i32>) -> Vec<usize> {
    let rewards_d: Vec<i32> = rewards.iter().map(|r| *r.max(&0)).collect();

    if rewards_d.iter().all(|r| *r == 0) {
        rewards_d.iter().map(|_r| 1).collect()
    } else {
        rewards_d.iter().map(|r| *r as usize).collect()
    }
}

fn mcts_ply(state: &State, player_idx: usize) -> Action {
    let mut rng = rand::rng();

    let actions = list_valid_actions(state, player_idx);
    let rewards: Vec<i32> = actions.iter().map(|&a| calculate_reward(state, player_idx, a)).collect();
    // Normalize rewards to return positive numbers
    let rewards_d = rewards_dist(rewards.clone());

    let epsilon = 0.05;
    let action_idx = if rng.random_range(0.0..1.0) < epsilon {
        (0..actions.len()).choose(&mut rng).unwrap()
    } else {
        let dist = WeightedIndex::new(&rewards_d).unwrap();
        dist.sample(&mut rng)
    };

    actions[action_idx]
}

// Run MCTS guided by immediate scores
pub fn play_mcts(state: &mut State, player_idx: usize) {
    let n_games = 200;
    let mut rng = rand::rng();

    let actions = list_valid_actions(state, player_idx);
    let rewards: Vec<i32> = actions.iter().map(|&a| calculate_reward(state, player_idx, a.clone())).collect();
    // Normalize rewards to return positive numbers
    let rewards_d = rewards_dist(rewards.clone());

    // Scores, wins, total expansions
    let mut action_log: Vec<(Vec<usize>, usize, usize)> = vec![(Vec::new(), 0, 0); actions.len()];

    let mut dist: WeightedIndex<usize>;

    let epsilon = 0.05;

    for _ in 0..n_games {
        let mut future_state = state.clone();
        let action_idx: usize;

        if rng.random_range(0.0..1.0) < epsilon {
            action_idx = (0..actions.len()).choose(&mut rng).unwrap();
        } else {
            dist = WeightedIndex::new(&rewards_d).unwrap();
            action_idx = dist.sample(&mut rng);
        }

        take_action(&mut future_state, player_idx, actions[action_idx]);
        let mut next_player_idx = player_idx;

        log::debug!("Scores after first move: {:?}", future_state.players.iter().map(|p| p.score).collect::<Vec<_>>());

        // Now we keep rolling till the game is complete. This implementation
        // doesn't do caching so it will not be super efficient nor effective.
        // Every player does a weighted sampling over possible next action
        // rewards to decide their action.
        loop {
            if future_state.is_game_over() {
                break;
            }

            if future_state.is_round_over() {
                future_state.rounds += 1;
                for i in 0..future_state.players.len() {
                    tile_wall_and_score(&mut future_state, i);
                }
                refill_tiles(&mut future_state);
            }

            next_player_idx += 1;
            next_player_idx %= future_state.players.len();

            let next_action = mcts_ply(&future_state, next_player_idx);
            take_action(&mut future_state, next_player_idx, next_action);
        }

        log::debug!("Scores after game end: {:?}", future_state.players.iter().map(|p| p.score).collect::<Vec<_>>());

        // One MC game is over, update the log
        let mut scores = action_log[action_idx].0.clone();
        scores.push(future_state.players[player_idx].score as usize);

        action_log[action_idx] = (
            scores,
            action_log[action_idx].1 + ((player_idx == winner(&future_state)) as usize),
            action_log[action_idx].2 + 1
        );
    }

    let best_action_idx = action_log
        .iter()
        .enumerate()
        .filter(|(_i, (_scores, _n_wins, n_games))| *n_games > 0)
        .max_by(|(_i, (scores_i, _n_wins_i, _n_games_i)), (_j, (scores_j, _n_wins_j, _n_games_j))| {
            let max_score_i = *scores_i.iter().max().unwrap();
            let max_score_j = *scores_j.iter().max().unwrap();

            max_score_i.cmp(&max_score_j)
        })
        .unwrap()
        .0;

    log::info!("Picked {:?}", action_log[best_action_idx]);

    take_action(state, player_idx, actions[best_action_idx]);
}
