use std::{collections::HashMap, vec};
use rand::seq::{IndexedRandom, IteratorRandom};
use std::collections::HashSet;
use std::convert::TryInto;

use anyhow::{anyhow, Result};

mod games;

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

#[derive(Debug)]
struct CenterState {
    tiles: HashMap<Tile, usize>,
    starting_marker: bool,
}

#[derive(Clone, Debug)]
struct PlayerState {
    score: i32,
    wall: [[bool; 5]; 5],
    pattern_lines: [(Option<Tile>, usize); 5],
    floor_line: usize,
    starting_marker: bool,
}

#[derive(Debug)]
struct State {
    factory_displays: Vec<FactoryDisplayState>,
    center: CenterState,
    players: Vec<PlayerState>,
    rounds: usize,
}

trait Validate {
    fn validate(&self) -> Result<()>;
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

    fn is_complete(&self) -> bool {
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

impl State {
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

    // Tell if the tiles are empty
    fn has_no_tiles(&self) -> bool {
        self.center.has_no_tiles() && self.factory_displays
            .iter()
            .map(|d| has_no_tiles(d.clone()))
            .any(|x| x)
    }
}

// Refill tiles in factory_displays, resetting center
fn refill_tiles(state: &mut State) {
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

// Action that tells which tile stash is picked by a player
#[derive(Clone, Copy, Debug)]
enum ActionDisplay {
    FactoryDisplay(usize),
    Center
}

fn has_no_tiles(display: FactoryDisplayState) -> bool {
    display.values().sum::<usize>() == 0
}

fn pick_random_display(state: &State) -> Option<ActionDisplay> {
    let mut rng = rand::rng();

    // Check which displays have items
    let active_displays: Vec<(usize, FactoryDisplayState)> = state.factory_displays
        .clone()
        .into_iter()
        .enumerate()
        .filter(|(_i, x)| !has_no_tiles(x.clone()))
        .map(|ix| ix)
        .collect();

    if !state.center.has_no_tiles() {
        let idx = rand::random_range(..(active_displays.len() + 1));
        if idx == active_displays.len() {
            Some(ActionDisplay::Center)
        } else {
            Some(ActionDisplay::FactoryDisplay(active_displays[idx].0))
        }
    } else {
        if active_displays.is_empty() {
            None
        } else {
            let (selected_display_idx, _) = active_displays.choose(&mut rng).unwrap();
            Some(ActionDisplay::FactoryDisplay(*selected_display_idx))
        }
    }
}

fn pick_color(state: &State, action: ActionDisplay) -> Tile {
    let mut rng = rand::rng();

    let colors: HashSet<Tile> = match action {
        ActionDisplay::Center => state.center.tiles.iter(),
        ActionDisplay::FactoryDisplay(i) => state.factory_displays[i].iter(),
    }
        .filter_map(|(&tile, &count)| (count > 0).then_some(tile))
        .collect();

    colors.into_iter().choose(&mut rng).unwrap()
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

fn place_tiles_random(state: &mut State, tiles: Vec<Tile>, player_idx: usize) {
    // All tiles are of the same color
    let color = tiles[0];
    let mut rng = rand::rng();

    let empty_lines = find_empty_lines(state, color, player_idx);

    if empty_lines.is_empty() {
        state.players[player_idx].floor_line += tiles.len();
        state.players[player_idx].floor_line = std::cmp::min(state.players[player_idx].floor_line, 7);
    } else {
        let line_idx = empty_lines.choose(&mut rng).unwrap();
        let line_size = line_idx + 1;
        let space = line_size - state.players[player_idx].pattern_lines[*line_idx].1;

        if space < tiles.len() {
            state.players[player_idx].pattern_lines[*line_idx] = (Some(color), space);
            // Penalize for the leftovers
            state.players[player_idx].floor_line += tiles.len() - space;
            state.players[player_idx].floor_line = std::cmp::min(state.players[player_idx].floor_line, 7);
        } else {
            state.players[player_idx].pattern_lines[*line_idx] = (Some(color), tiles.len());
        }
    }
}

fn play_random(state: &mut State, player_idx: usize) {
    // Pick the place for taking tiles
    let action = pick_random_display(state).expect("No tiles left");
    let color = pick_color(state, action);

    let tiles = take_out_tiles(state, action, color);
    // In case the action involves picking from center, take the starting marker
    // if not already taken
    if let ActionDisplay::Center = action {
        if state.center.starting_marker {
            state.players[player_idx].starting_marker = true;
            state.center.starting_marker = false
        }
    }

    place_tiles_random(state, tiles, player_idx);
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
    let color_completed: bool = false;
    // TODO
    if color_completed {
        score += 10
    }

    score
}

fn execute_placement(wall: &mut [[bool; 5]; 5], row_idx: usize, color: Tile) {
    let col_idx = WALL_COLORS[row_idx].iter().position(|&x| x == color).unwrap();
    wall[row_idx][col_idx] = true;
}

// Build the wall and score players
fn score(state: &mut State, player_idx: usize) {
    let mut accumulator: i32 = 0;

    let mut tiling_points = 0;
    for i in 0..5 {
        let line_size = i + 1;
        if state.players[player_idx].pattern_lines[i].1 == line_size {
            let color = state.players[player_idx].pattern_lines[i].0.unwrap();
            tiling_points += score_placement(&state.players[player_idx].wall, i, color);
            execute_placement(&mut state.players[player_idx].wall, i, color);
            state.players[player_idx].pattern_lines[i] = (None, 0);
        }
    }
    accumulator += tiling_points;
    log::debug!("P{} got {} in tiling", player_idx, tiling_points);

    // Take penalties, if any
    let penalties = FLOOR_PENALTIES.iter().take(state.players[player_idx].floor_line).sum::<usize>() as i32;
    accumulator -= penalties;
    log::debug!("P{} lost {} as penalties", player_idx, penalties);

    state.players[player_idx].score += accumulator;
    // state.players[player_idx].score = std::cmp::max(state.players[player_idx].score, 0);

    state.players[player_idx].floor_line = 0;
}

// Game is over if at least one player has completed their setup
fn is_game_over(state: &State) -> bool {
    for player in state.players.clone() {
        if player.is_complete() {
            return true
        }
    }

    false
}

// Tell if one of the player has starting marker
fn first_player(state: &State) -> Option<usize> {
    for i in 0..state.players.len() {
        if state.players[i].starting_marker {
            return Some(i)
        }
    }

    None
}

fn winner(state: &State) -> usize {
    state.players
        .iter()
        .enumerate()
        .max_by_key(|(_i, p)| p.score)
        .unwrap()
        .0
}

fn main() {
    env_logger::init();

    let n_players: usize = 3;
    let mut state = State::new(n_players);

    if let Err(err) = state.validate() {
        println!("{}", err);
        return;
    }

    loop {
        log::debug!("Round: {}", state.rounds);
        let mut current_player = match first_player(&state) {
            Some(one) => {
                state.players[one].starting_marker = false;
                one
            },
            None => 0,
        };

        log::debug!("Starting player: {}", current_player);

        refill_tiles(&mut state);
        loop {
            // If tiles are over, round stops
            if state.has_no_tiles() {
                state.rounds += 1;
                break;
            }
            play_random(&mut state, current_player);
            current_player += 1;
            current_player %= n_players;
        }

        for i in 0..n_players {
            score(&mut state, i);
            log::debug!("Score P{}: {}", i, state.players[i].score);
        }

        if is_game_over(&state) {
            break;
        }
    }

    for i in 0..n_players {
        log::info!("Final score P{}: {}", i, state.players[i].score);
    }

    log::info!("Winner is P{}", winner(&state));
}
