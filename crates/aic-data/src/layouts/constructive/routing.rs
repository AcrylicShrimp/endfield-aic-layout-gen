use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

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
    let mut open = BinaryHeap::new();
    let mut best = HashMap::from([(initial, Cost { steps: 0, turns: 0 })]);
    let mut predecessor = HashMap::<State, State>::new();
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
            return Some(reconstruct(entry.state, initial, &predecessor));
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
            if best.get(&next).is_some_and(|known| *known <= next_cost) {
                continue;
            }
            best.insert(next, next_cost);
            predecessor.insert(next, entry.state);
            open.push(QueueEntry {
                estimated_steps: next_cost.steps + manhattan(next_x, next_y, goal.x, goal.y),
                turns: next_cost.turns,
                steps: next_cost.steps,
                state: next,
            });
        }
    }
    None
}

fn inside(width: i64, height: i64, x: i64, y: i64) -> bool {
    x >= 0 && y >= 0 && x < width && y < height
}

fn manhattan(x: i64, y: i64, goal_x: i64, goal_y: i64) -> usize {
    usize::try_from(x.abs_diff(goal_x) + y.abs_diff(goal_y)).unwrap_or(usize::MAX)
}

fn reconstruct(
    mut state: State,
    initial: State,
    predecessor: &HashMap<State, State>,
) -> Vec<WorldGridPosition> {
    let mut path = vec![WorldGridPosition {
        x: state.x,
        y: state.y,
    }];
    while state != initial {
        state = predecessor[&state];
        path.push(WorldGridPosition {
            x: state.x,
            y: state.y,
        });
    }
    path.reverse();
    path
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
}
