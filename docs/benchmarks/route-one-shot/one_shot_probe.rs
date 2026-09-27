use std::hint::black_box;
use std::time::Instant;
use aic_layout::geometry::{Bounds, Point};
use aic_routing::{Crossings, PathOptions, shortest_path};
use indexmap::{IndexMap, IndexSet};

fn measure(name: &str, bounds: Bounds, blocked: IndexSet<Point>, penalties: IndexMap<Point,u32>, start: Point, end: Point) {
    let crossings = Crossings::new();
    let mut elapsed = Vec::new();
    let mut signature = String::new();
    for iteration in 0..9 {
        let began = Instant::now();
        let result = shortest_path(
            black_box(start), black_box(end), &blocked, bounds,
            PathOptions {
                first: None, last: None, work_budget: 200_000,
                scan_budget: 2_000_000, frontier_budget: 200_000,
                deadline: None, crossings: &crossings, crossing_limit: 0,
                prefer_crossings: false, allow_zero_length: false,
                cell_penalties: Some(&penalties), edge_penalties: None,
            }
        ).unwrap();
        let nanos = began.elapsed().as_nanos();
        let mut hash: u64 = 1469598103934665603;
        if let Some(cells) = &result.cells {
            for p in cells {
                hash ^= p.x as u64;
                hash = hash.wrapping_mul(1099511628211);
                hash ^= p.y as u64;
                hash = hash.wrapping_mul(1099511628211);
            }
        }
        signature = format!("{}:{}:{}:{}:{}:{}:{}:{}:{hash}",
            result.status, result.steps.unwrap_or(-1), result.turns.unwrap_or(-1),
            result.expanded, result.preflight_expanded, result.routing_scans,
            result.frontier_entries_peak, result.preflight_status);
        if iteration != 0 { elapsed.push(nanos); }
    }
    elapsed.sort();
    println!("{name},{},{signature}",elapsed[elapsed.len()/2]);
}

fn main() {
    let bounds = Bounds { min_x:0,min_y:0,max_x:127,max_y:127 };
    let mut blocked = IndexSet::new();
    let mut penalties = IndexMap::new();
    for y in 0..128 {
        for x in 0..128 {
            if (x*37+y*19)%31 == 0 && x>2 && y>2 && x<125 && y<125 {
                blocked.insert(Point{x,y});
            }
            if (x*11+y*17)%29 == 0 {
                penalties.insert(Point{x,y},3);
            }
        }
    }
    measure("dense128",bounds,blocked,penalties,Point{x:0,y:0},Point{x:127,y:127});

    let bounds = Bounds { min_x:0,min_y:0,max_x:299,max_y:299 };
    let mut blocked = IndexSet::new();
    let mut penalties = IndexMap::new();
    for y in 0..300 {
        for x in 0..300 {
            if (x*41+y*73)%23 == 0 && x>2 && y>2 && x<297 && y<297 {
                blocked.insert(Point{x,y});
            }
            if (x*3+y*7)%17 == 0 {
                penalties.insert(Point{x,y},1);
            }
        }
    }
    measure("sparse300",bounds,blocked,penalties,Point{x:0,y:0},Point{x:299,y:299});
}
