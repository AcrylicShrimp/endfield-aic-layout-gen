use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::direction,
    logistics::TransportKind,
    physical::{Layout, Metrics},
    projection::rotate,
    validation::Catalogs,
};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write,
};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn svg(layout: &Layout, catalogs: &Catalogs) -> String {
    let mut right = 0;
    let mut bottom = 0;
    let mut projected = BTreeMap::new();
    for block in &layout.problem.blocks {
        let pose = layout
            .solution
            .poses
            .iter()
            .find(|p| p.block == block.id)
            .unwrap();
        let (w, h, ports) = rotate(
            catalogs.facilities.facility(&block.facility).unwrap(),
            pose.rotation,
            pose.position,
        );
        right = right.max(pose.position.x + w);
        bottom = bottom.max(pose.position.y + h);
        projected.insert(block.id.as_str(), (pose, w, h, ports));
    }
    let edges: BTreeMap<_, _> = layout
        .problem
        .connections
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let mut active_ports = HashSet::new();
    for route in &layout.solution.routes {
        for p in &route.cells {
            right = right.max(p.x + 1);
            bottom = bottom.max(p.y + 1);
        }
    }
    let mut out = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-1 -1 {} {}" role="img" aria-label="Routed procedural layout"><defs><pattern id="grid" width="1" height="1" patternUnits="userSpaceOnUse"><path d="M1 0H0V1" fill="none" stroke="#253347" stroke-width=".035"/></pattern></defs><rect x="-1" y="-1" width="100%" height="100%" fill="#101a28"/><rect width="{}" height="{}" fill="url(#grid)"/>"##,
        right + 3,
        bottom + 3,
        right + 2,
        bottom + 2
    );
    for route in &layout.solution.routes {
        let edge = edges[route.connection.as_str()];
        let source_port = route
            .source_port
            .as_deref()
            .expect("procedural generator routes have facility sources");
        let target_port = route
            .target_port
            .as_deref()
            .expect("procedural generator routes have facility targets");
        active_ports.insert((edge.source.as_str(), source_port));
        active_ports.insert((edge.target.as_str(), target_port));
        let source = projected[edge.source.as_str()]
            .3
            .iter()
            .find(|p| p.id == source_port)
            .unwrap();
        let target = projected[edge.target.as_str()]
            .3
            .iter()
            .find(|p| p.id == target_port)
            .unwrap();
        let (color, kind) = match edge.transport {
            TransportKind::Belt => ("#ffc56d", "belt"),
            TransportKind::Pipe => ("#68d8ef", "pipe"),
        };
        let points = std::iter::once(&source.position)
            .chain(&route.cells)
            .chain(std::iter::once(&target.position))
            .map(|p| format!("{},{}", p.x as f64 + 0.5, p.y as f64 + 0.5))
            .collect::<Vec<_>>()
            .join(" ");
        write!(out, r##"<g class="route {kind}" data-edge="{}"><title>{}: {} → {} · {}/s · {} cells</title><polyline points="{points}" fill="none" stroke="{color}" stroke-width=".34" stroke-linecap="round" stroke-linejoin="round"/>"##, escape(&edge.id), escape(&edge.id), escape(&edge.source), escape(&edge.target), edge.rate.numerator as f64 / edge.rate.denominator as f64, route.cells.len()).unwrap();
        if route.cells.len() > 1 {
            let at = route.cells.len() / 2;
            let p = route.cells[at];
            let d = direction(route.cells[at - 1], p).unwrap();
            write!(out, r##"<path d="M-.27-.24 L.26 0 L-.27.24" transform="translate({} {}) rotate({})" fill="none" stroke="#101a28" stroke-width=".12"/></g>"##, p.x as f64 + 0.5, p.y as f64 + 0.5, d * 90).unwrap();
        } else {
            out.push_str("</g>");
        }
    }
    for bridge in &layout.solution.bridges {
        write!(out, r##"<g class="bridge"><title>{}: {}</title><rect x="{}" y="{}" width=".74" height=".74" rx=".12" fill="#101a28" stroke="#edf1fa" stroke-width=".1"/><path d="M{} {}h.55" stroke="#edf1fa" stroke-width=".11"/></g>"##, escape(&bridge.id), escape(&bridge.component), bridge.position.x as f64 + 0.13, bridge.position.y as f64 + 0.13, bridge.position.x as f64 + 0.22, bridge.position.y as f64 + 0.5).unwrap();
    }
    for block in &layout.problem.blocks {
        let (pose, w, h, ports) = &projected[block.id.as_str()];
        let (w, h) = (*w, *h);
        let x = pose.position.x;
        let y = pose.position.y;
        write!(out, r##"<g class="block"><title>{} · {} · {}×{} · {}°</title><rect x="{}" y="{}" width="{}" height="{}" rx=".18" fill="#293b50" stroke="#8199b5" stroke-width=".10"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="middle" fill="#eaf1fb" font-family="system-ui,sans-serif" font-size=".85">{}</text></g>"##, escape(&block.id), escape(&block.facility), w,h,pose.rotation, x as f64 + 0.08,y as f64 + 0.08,w as f64 - 0.16,h as f64 - 0.16,x as f64 + w as f64/2.0,y as f64 + h as f64/2.0,escape(&block.id.replace("block-", "B"))).unwrap();
        for p in ports {
            let active = active_ports.contains(&(block.id.as_str(), p.id.as_str()));
            let color = if !active {
                "#52667e"
            } else if p.direction == FacilityPortDirection::Output {
                "#e8ad65"
            } else {
                "#7dd4c1"
            };
            write!(out, r##"<circle class="port" cx="{}" cy="{}" r=".16" fill="{color}"><title>{}: {} {:?}</title></circle>"##, p.position.x as f64 + 0.5,p.position.y as f64 + 0.5,escape(&block.id),escape(&p.id),p.direction).unwrap();
        }
    }
    out.push_str("</svg>");
    out
}

pub fn page(svg: &str, seed: u64, metrics: &Metrics) -> String {
    format!(
        r##"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Layout {seed}</title><style>body{{margin:0;background:#0b121c;color:#eaf1fb;font:15px system-ui}}header{{padding:20px 28px;display:flex;align-items:center;gap:24px;flex-wrap:wrap;border-bottom:1px solid #293b50}}h1{{font-size:20px;margin:0}}a{{color:#a4c8ef}}main{{padding:16px;height:calc(100vh - 130px)}}svg{{width:100%;height:100%}}.route{{cursor:pointer}}svg.selected .route{{opacity:.12}}svg.selected .route.focus{{opacity:1}}label{{cursor:pointer}}small{{color:#9eafc3}}</style><header><a href="index.html">← Gallery</a><h1>Seed {seed}</h1><small>{} × {} · {} cells · {:.1}% occupied · {} bridges</small><label><input id="belts" type="checkbox" checked> Belts</label><label><input id="pipes" type="checkbox" checked> Pipes</label><label><input id="ports" type="checkbox" checked> Ports</label><small>Click a route to isolate it; click again to reset.</small></header><main>{svg}</main><script>for(const [id,cls] of [['belts','belt'],['pipes','pipe'],['ports','port']])document.getElementById(id).onchange=e=>document.querySelectorAll('.'+cls).forEach(n=>n.style.display=e.target.checked?'':'none');const svg=document.querySelector('svg');svg.onclick=e=>{{const route=e.target.closest('.route');const on=route&&!route.classList.contains('focus');svg.querySelectorAll('.focus').forEach(n=>n.classList.remove('focus'));svg.classList.toggle('selected',!!on);if(on)route.classList.add('focus');}};</script></html>"##,
        metrics.width,
        metrics.height,
        metrics.area,
        metrics.occupancy * 100.0,
        metrics.bridges
    )
}

pub fn gallery(rows: &[crate::batch::Row], elapsed: f64) -> String {
    let complete = rows.iter().filter(|r| r.metrics.is_some()).count();
    let mut out = format!(
        r##"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Procedural layout gallery</title><style>*{{box-sizing:border-box}}body{{margin:0;background:#0b121c;color:#eaf1fb;font:15px system-ui}}header{{padding:32px 36px;border-bottom:1px solid #293b50}}h1{{font-size:28px;margin:0 0 12px}}p{{color:#9eafc3;margin:8px 0}}main{{padding:24px 36px;display:grid;grid-template-columns:repeat(auto-fill,minmax(340px,1fr));gap:20px}}article{{background:#142030;border:1px solid #293b50;border-radius:12px;overflow:hidden}}img{{display:block;width:100%;height:280px;object-fit:contain;background:#101a28}}.details{{padding:16px}}h2{{font-size:17px;margin:0 0 10px}}a{{color:inherit;text-decoration:none}}small{{color:#9eafc3}}.stats{{display:flex;gap:14px;flex-wrap:wrap;color:#9eafc3;font-size:13px}}.fail{{color:#f0b0a0}}code{{color:#9ac1ef}}button{{padding:8px 14px;background:#293b50;color:#eaf1fb;border:1px solid #52667e;border-radius:6px;cursor:pointer;margin-right:8px}}</style><header><h1>Procedural routed layouts</h1><p>{complete} / {} complete · {:.2}s total · {:.2} layouts/s including validation and output</p><p>Seeded sequence pairs · catalog-backed ports and bridges · independent physical validation</p><p>Amber: belts · Cyan: pipes · White squares: bridges. Select a layout to inspect its routes.</p><button onclick="filter('all')">All</button><button onclick="filter('complete')">Complete</button><button onclick="filter('failed')">Failed</button></header><main>"##,
        rows.len(),
        elapsed,
        complete as f64 / elapsed.max(1e-9)
    );
    for r in rows {
        if let Some(m) = &r.metrics {
            write!(out, r##"<article data-status="complete"><a href="case-{:05}.html"><img src="case-{:05}.svg" alt="Layout for seed {}" loading="lazy"><div class="details"><h2>Seed {}</h2><div class="stats"><span>{} blocks / {} links</span><span>{} × {}</span><span>{:.1}% occupied</span><span>{} branches / {} merges</span><span>{} bridges</span></div><p><small>{:.1}ms generation · {} attempt(s)</small></p></div></a></article>"##,r.index,r.index,r.seed,r.seed,r.blocks,r.connections,m.width,m.height,m.occupancy*100.0,m.branch_blocks,m.merge_blocks,m.bridges,r.generation_ms,r.attempts.len()).unwrap();
        } else {
            write!(out,r##"<article data-status="failed"><div class="details"><h2>Seed {}</h2><p class="fail">Generation exhausted</p><p>{} attempt(s). No partial layout published.</p><a href="case-{:05}.json">Diagnostics →</a></div></article>"##,r.seed,r.attempts.len(),r.index).unwrap();
        }
    }
    out.push_str("</main><script>function filter(s){document.querySelectorAll('article').forEach(a=>a.hidden=s!=='all'&&a.dataset.status!==s)}</script></html>");
    out
}
