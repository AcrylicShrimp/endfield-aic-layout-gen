use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use crate::layouts::WorldGridPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    x: i64,
    y: i64,
    heading: Option<Direction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    fn delta(self) -> (i64, i64) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }

    fn index(self) -> usize {
        usize::from(self.rank())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cost {
    steps: usize,
    turns: usize,
}

impl Ord for Cost {
    fn cmp(&self, other: &Self) -> Ordering {
        self.steps
            .cmp(&other.steps)
            .then_with(|| self.turns.cmp(&other.turns))
    }
}

impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QueueEntry {
    estimated_steps: usize,
    turns: usize,
    steps: usize,
    state: State,
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimated_steps
            .cmp(&self.estimated_steps)
            .then_with(|| other.turns.cmp(&self.turns))
            .then_with(|| other.steps.cmp(&self.steps))
            .then_with(|| other.state.y.cmp(&self.state.y))
            .then_with(|| other.state.x.cmp(&self.state.x))
            .then_with(|| {
                other
                    .state
                    .heading
                    .map(Direction::rank)
                    .cmp(&self.state.heading.map(Direction::rank))
            })
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn route_shortest_path(
    width: i64,
    height: i64,
    blocked: &HashSet<(i64, i64)>,
    start: &WorldGridPosition,
    goal: &WorldGridPosition,
) -> Option<Vec<WorldGridPosition>> {
    RouteWorkspace::new(width, height).route(blocked, start, goal)
}

pub(super) struct RouteWorkspace {
    width: i64,
    height: i64,
    generation: u32,
    visited_generation: Vec<u32>,
    best: Vec<Cost>,
    predecessor: Vec<State>,
    open: BinaryHeap<QueueEntry>,
}

impl RouteWorkspace {
    pub(super) fn new(width: i64, height: i64) -> Self {
        let cells = usize::try_from(width.max(0))
            .unwrap_or(0)
            .saturating_mul(usize::try_from(height.max(0)).unwrap_or(0));
        let states = cells.saturating_mul(Direction::ALL.len());
        Self {
            width,
            height,
            generation: 0,
            visited_generation: vec![0; states],
            best: vec![Cost { steps: 0, turns: 0 }; states],
            predecessor: vec![
                State {
                    x: 0,
                    y: 0,
                    heading: None,
                };
                states
            ],
            open: BinaryHeap::new(),
        }
    }

    pub(super) fn route(
        &mut self,
        blocked: &HashSet<(i64, i64)>,
        start: &WorldGridPosition,
        goal: &WorldGridPosition,
    ) -> Option<Vec<WorldGridPosition>> {
        let width = self.width;
        let height = self.height;
        if !inside(width, height, start.x, start.y)
            || !inside(width, height, goal.x, goal.y)
            || blocked.contains(&(start.x, start.y))
            || blocked.contains(&(goal.x, goal.y))
        {
            return None;
        }
        if start == goal {
            return Some(vec![start.clone()]);
        }

        let initial = State {
            x: start.x,
            y: start.y,
            heading: None,
        };
        self.begin_search();
        self.open.push(QueueEntry {
            estimated_steps: manhattan(start.x, start.y, goal.x, goal.y),
            turns: 0,
            steps: 0,
            state: initial,
        });

        while let Some(entry) = self.open.pop() {
            let cost = Cost {
                steps: entry.steps,
                turns: entry.turns,
            };
            if entry.state.heading.is_some()
                && self
                    .known_cost(entry.state)
                    .is_some_and(|known| known < cost)
            {
                continue;
            }
            if entry.state.x == goal.x && entry.state.y == goal.y {
                return Some(self.reconstruct(entry.state, initial));
            }

            for direction in Direction::ALL {
                let (dx, dy) = direction.delta();
                let next_x = entry.state.x + dx;
                let next_y = entry.state.y + dy;
                if !inside(width, height, next_x, next_y) || blocked.contains(&(next_x, next_y)) {
                    continue;
                }
                let next = State {
                    x: next_x,
                    y: next_y,
                    heading: Some(direction),
                };
                let next_cost = Cost {
                    steps: cost.steps + 1,
                    turns: cost.turns
                        + usize::from(entry.state.heading.is_some_and(|last| last != direction)),
                };
                if self
                    .known_cost(next)
                    .is_some_and(|known| known <= next_cost)
                {
                    continue;
                }
                self.record(next, next_cost, entry.state);
                self.open.push(QueueEntry {
                    estimated_steps: next_cost.steps + manhattan(next_x, next_y, goal.x, goal.y),
                    turns: next_cost.turns,
                    steps: next_cost.steps,
                    state: next,
                });
            }
        }
        None
    }

    fn begin_search(&mut self) {
        self.open.clear();
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited_generation.fill(0);
            self.generation = 1;
        }
    }

    fn state_index(&self, state: State) -> usize {
        let direction = state
            .heading
            .expect("only directional routing states are indexed");
        let cell = usize::try_from(state.y * self.width + state.x)
            .expect("routing state must be inside the workspace");
        cell * Direction::ALL.len() + direction.index()
    }

    fn known_cost(&self, state: State) -> Option<Cost> {
        let index = self.state_index(state);
        (self.visited_generation[index] == self.generation).then_some(self.best[index])
    }

    fn record(&mut self, state: State, cost: Cost, predecessor: State) {
        let index = self.state_index(state);
        self.visited_generation[index] = self.generation;
        self.best[index] = cost;
        self.predecessor[index] = predecessor;
    }

    fn reconstruct(&self, mut state: State, initial: State) -> Vec<WorldGridPosition> {
        let mut path = vec![WorldGridPosition {
            x: state.x,
            y: state.y,
        }];
        while state != initial {
            state = self.predecessor[self.state_index(state)];
            path.push(WorldGridPosition {
                x: state.x,
                y: state.y,
            });
        }
        path.reverse();
        path
    }
}

fn inside(width: i64, height: i64, x: i64, y: i64) -> bool {
    x >= 0 && y >= 0 && x < width && y < height
}

fn manhattan(x: i64, y: i64, goal_x: i64, goal_y: i64) -> usize {
    usize::try_from(x.abs_diff(goal_x) + y.abs_diff(goal_y)).unwrap_or(usize::MAX)
}

pub(super) fn count_turns(path: &[WorldGridPosition]) -> usize {
    path.windows(3)
        .filter(|window| {
            let first = (&window[1].x - &window[0].x, &window[1].y - &window[0].y);
            let second = (&window[2].x - &window[1].x, &window[2].y - &window[1].y);
            first != second
        })
        .count()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn astar_prefers_fewer_turns_among_shortest_paths() {
        let path = route_shortest_path(
            5,
            5,
            &HashSet::new(),
            &WorldGridPosition { x: 0, y: 0 },
            &WorldGridPosition { x: 3, y: 2 },
        )
        .expect("open grid should route");
        assert_eq!(path.len(), 6);
        assert_eq!(count_turns(&path), 1);
    }

    #[test]
    fn astar_routes_around_facility_cells() {
        let blocked = HashSet::from([(1, 0), (1, 1), (1, 2)]);
        let path = route_shortest_path(
            4,
            4,
            &blocked,
            &WorldGridPosition { x: 0, y: 1 },
            &WorldGridPosition { x: 2, y: 1 },
        )
        .expect("path should go around the wall");
        assert!(path.iter().all(|cell| !blocked.contains(&(cell.x, cell.y))));
        assert_eq!(path.len(), 7);
        assert_eq!(count_turns(&path), 2);
    }

    #[test]
    fn dense_workspace_matches_hash_map_reference_costs() {
        let mut random = 0x6a09_e667_f3bc_c909_u64;
        for _ in 0..200 {
            let width = 3 + i64::try_from(next_random(&mut random) % 8).unwrap();
            let height = 3 + i64::try_from(next_random(&mut random) % 8).unwrap();
            let start = WorldGridPosition {
                x: i64::try_from(next_random(&mut random) % u64::try_from(width).unwrap()).unwrap(),
                y: i64::try_from(next_random(&mut random) % u64::try_from(height).unwrap())
                    .unwrap(),
            };
            let goal = WorldGridPosition {
                x: i64::try_from(next_random(&mut random) % u64::try_from(width).unwrap()).unwrap(),
                y: i64::try_from(next_random(&mut random) % u64::try_from(height).unwrap())
                    .unwrap(),
            };
            let mut blocked = HashSet::new();
            for y in 0..height {
                for x in 0..width {
                    if next_random(&mut random) % 4 == 0 {
                        blocked.insert((x, y));
                    }
                }
            }
            blocked.remove(&(start.x, start.y));
            blocked.remove(&(goal.x, goal.y));

            let expected = reference_route_cost(width, height, &blocked, &start, &goal);
            let actual = RouteWorkspace::new(width, height)
                .route(&blocked, &start, &goal)
                .map(|path| Cost {
                    steps: path.len() - 1,
                    turns: count_turns(&path),
                });
            assert_eq!(actual, expected, "mismatch on {width}x{height} grid");
        }
    }

    fn next_random(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }

    fn reference_route_cost(
        width: i64,
        height: i64,
        blocked: &HashSet<(i64, i64)>,
        start: &WorldGridPosition,
        goal: &WorldGridPosition,
    ) -> Option<Cost> {
        if start == goal {
            return Some(Cost { steps: 0, turns: 0 });
        }
        let initial = State {
            x: start.x,
            y: start.y,
            heading: None,
        };
        let mut open = BinaryHeap::new();
        let mut best = HashMap::from([(initial, Cost { steps: 0, turns: 0 })]);
        open.push(QueueEntry {
            estimated_steps: manhattan(start.x, start.y, goal.x, goal.y),
            turns: 0,
            steps: 0,
            state: initial,
        });
        while let Some(entry) = open.pop() {
            let cost = Cost {
                steps: entry.steps,
                turns: entry.turns,
            };
            if best.get(&entry.state).is_some_and(|known| *known < cost) {
                continue;
            }
            if entry.state.x == goal.x && entry.state.y == goal.y {
                return Some(cost);
            }
            for direction in Direction::ALL {
                let (dx, dy) = direction.delta();
                let next = State {
                    x: entry.state.x + dx,
                    y: entry.state.y + dy,
                    heading: Some(direction),
                };
                if !inside(width, height, next.x, next.y) || blocked.contains(&(next.x, next.y)) {
                    continue;
                }
                let next_cost = Cost {
                    steps: cost.steps + 1,
                    turns: cost.turns
                        + usize::from(entry.state.heading.is_some_and(|last| last != direction)),
                };
                if best.get(&next).is_some_and(|known| *known <= next_cost) {
                    continue;
                }
                best.insert(next, next_cost);
                open.push(QueueEntry {
                    estimated_steps: next_cost.steps + manhattan(next.x, next.y, goal.x, goal.y),
                    turns: next_cost.turns,
                    steps: next_cost.steps,
                    state: next,
                });
            }
        }
        None
    }
}
