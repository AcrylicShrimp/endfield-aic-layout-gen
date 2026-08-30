use std::fmt::Write;

use crate::logistics::{LogisticsComponentKind, TransportKind};

use super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedRouteEndpoint,
    PlacedLogisticsComponent,
};

pub fn render_integrated_layout_html(
    report: &IntegratedLayoutReport,
) -> Result<String, IntegratedLayoutDiagnostic> {
    if !report.success {
        return Err(IntegratedLayoutDiagnostic::error(
            "layout-visualization-requires-success",
            "/",
            None,
            "layout visualization requires a successful integrated layout report",
        ));
    }
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "layout-visualization-missing-bounds",
            "/bounds",
            None,
            "successful integrated layout report has no used bounds",
        )
    })?;
    if bounds.width < 0 || bounds.height < 0 {
        return Err(IntegratedLayoutDiagnostic::error(
            "layout-visualization-invalid-bounds",
            "/bounds",
            None,
            format!(
                "layout visualization requires non-negative used bounds, found {} by {}",
                bounds.width, bounds.height
            ),
        ));
    }

    let width = bounds.width.max(1);
    let height = bounds.height.max(1);
    let route_cells = report
        .routes
        .iter()
        .map(|route| route.cells.len())
        .sum::<usize>();
    let belt_routes = report
        .routes
        .iter()
        .filter(|route| route.transport == TransportKind::Belt)
        .count();
    let pipe_routes = report.routes.len() - belt_routes;
    let bridge_count = report
        .logistics_components
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Bridge)
        .count();

    let mut html = String::with_capacity(route_cells.saturating_mul(10).max(32_768));
    write!(
        html,
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>AIC Layout Wireframe</title>
<style>
  :root {{ color-scheme: dark; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; background: #071019; color: #dbeeff; overflow: hidden; }}
  #aic-layout-viewer {{ height: 100vh; display: grid; grid-template-rows: auto 1fr; }}
  .toolbar {{ display: flex; align-items: center; gap: 14px; min-height: 48px; padding: 8px 12px; border-bottom: 1px solid #294153; background: #0b1722; flex-wrap: wrap; }}
  .title {{ color: #f2f8ff; font-weight: 700; letter-spacing: .04em; }}
  .metrics {{ color: #8ba8bd; font-size: 12px; margin-right: auto; }}
  button {{ appearance: none; border: 1px solid #35536a; background: transparent; color: #b9cee0; padding: 5px 8px; font: inherit; font-size: 12px; cursor: pointer; }}
  button[aria-pressed="false"] {{ color: #567083; border-color: #253c4d; text-decoration: line-through; }}
  button:hover, button:focus-visible {{ border-color: #8ecbff; color: #eaf6ff; outline: none; }}
  .swatch {{ display: inline-block; width: 16px; height: 0; margin-right: 5px; vertical-align: middle; border-top: 2px solid currentColor; }}
  .belt-swatch {{ color: #f1b84b; }}
  .pipe-swatch {{ color: #59c7f2; border-top-style: dashed; }}
  .stage {{ min-height: 0; position: relative; }}
  svg {{ display: block; width: 100%; height: 100%; background: #071019; touch-action: none; cursor: grab; }}
  svg.dragging {{ cursor: grabbing; }}
  .grid-minor {{ stroke: #122432; stroke-width: .06; }}
  .grid-major {{ stroke: #1d3547; stroke-width: .12; }}
  .route {{ fill: none; stroke-linejoin: round; stroke-linecap: square; vector-effect: non-scaling-stroke; }}
  .route-belt {{ stroke: #f1b84b; stroke-width: 1.15px; opacity: .58; }}
  .route-pipe {{ stroke: #59c7f2; stroke-width: 1.25px; stroke-dasharray: 3 2; opacity: .7; }}
  .route:hover {{ opacity: 1; stroke-width: 3px; }}
  .endpoint {{ vector-effect: non-scaling-stroke; stroke-width: 1px; }}
  .endpoint-belt {{ fill: #f1b84b; stroke: #251a07; }}
  .endpoint-pipe {{ fill: #59c7f2; stroke: #071c26; }}
  .boundary {{ fill: #071019; stroke: #ffec99; stroke-width: 1.5px; vector-effect: non-scaling-stroke; }}
  .facility {{ fill: #10293a; fill-opacity: .92; stroke: #d7efff; stroke-width: 1.25px; vector-effect: non-scaling-stroke; }}
  .facility:hover {{ fill: #19415a; stroke: #ffffff; stroke-width: 2.5px; }}
  .facility-label {{ fill: #dff3ff; font-size: .9px; text-anchor: middle; dominant-baseline: central; pointer-events: none; }}
  .component {{ fill: #071019; vector-effect: non-scaling-stroke; }}
  .bridge {{ stroke: #ff6b85; stroke-width: 1.5px; }}
  .splitter {{ stroke: #c4f06f; stroke-width: 1.5px; }}
  .converger {{ stroke: #d99cff; stroke-width: 1.5px; }}
  .hidden-layer {{ display: none; }}
  .help {{ position: absolute; right: 10px; bottom: 8px; color: #6f8b9e; font-size: 11px; pointer-events: none; }}
</style>
</head>
<body>
<div id="aic-layout-viewer">
  <div class="toolbar">
    <span class="title">AIC LAYOUT WIREFRAME</span>
    <span class="metrics">{}×{} · {} facilities · {} routes · {} route cells · {} bridges</span>
    <button type="button" data-toggle="belt-layer" aria-pressed="true"><span class="swatch belt-swatch"></span>Belt ({})</button>
    <button type="button" data-toggle="pipe-layer" aria-pressed="true"><span class="swatch pipe-swatch"></span>Pipe ({})</button>
    <button type="button" data-toggle="component-layer" aria-pressed="true">Components</button>
    <button type="button" data-toggle="label-layer" aria-pressed="true">Labels</button>
    <button type="button" data-reset>Reset view</button>
  </div>
  <div class="stage">
    <svg role="img" aria-label="AIC facility and logistics layout" viewBox="-2 -2 {} {}" data-base-view="-2 -2 {} {}">
      <defs>
        <pattern id="minor-grid" width="1" height="1" patternUnits="userSpaceOnUse"><path class="grid-minor" d="M 1 0 L 0 0 0 1" fill="none"/></pattern>
        <pattern id="major-grid" width="10" height="10" patternUnits="userSpaceOnUse"><rect width="10" height="10" fill="url(#minor-grid)"/><path class="grid-major" d="M 10 0 L 0 0 0 10" fill="none"/></pattern>
      </defs>
      <rect x="0" y="0" width="{}" height="{}" fill="url(#major-grid)" stroke="#4a6a80" stroke-width=".2"/>
"##,
        bounds.width,
        bounds.height,
        report.placements.len(),
        report.routes.len(),
        route_cells,
        bridge_count,
        belt_routes,
        pipe_routes,
        width + 4,
        height + 4,
        width + 4,
        height + 4,
        width,
        height,
    )
    .expect("writing to String cannot fail");

    render_routes(
        &mut html,
        report,
        TransportKind::Belt,
        "belt-layer",
        "route-belt",
    );
    render_routes(
        &mut html,
        report,
        TransportKind::Pipe,
        "pipe-layer",
        "route-pipe",
    );
    render_components(&mut html, &report.logistics_components);
    render_facilities(&mut html, report);

    html.push_str(
        r#"    </svg>
    <div class="help">wheel: zoom · drag: pan · hover: inspect</div>
  </div>
</div>
<script>
(() => {
  const root = document.getElementById('aic-layout-viewer');
  const svg = root.querySelector('svg');
  const base = svg.dataset.baseView.split(' ').map(Number);
  let view = base.slice();
  let drag = null;
  const applyView = () => svg.setAttribute('viewBox', view.join(' '));
  root.querySelectorAll('[data-toggle]').forEach((button) => {
    button.addEventListener('click', () => {
      const pressed = button.getAttribute('aria-pressed') === 'true';
      button.setAttribute('aria-pressed', String(!pressed));
      root.querySelector(`#${button.dataset.toggle}`).classList.toggle('hidden-layer', pressed);
    });
  });
  root.querySelector('[data-reset]').addEventListener('click', () => { view = base.slice(); applyView(); });
  svg.addEventListener('wheel', (event) => {
    event.preventDefault();
    const rect = svg.getBoundingClientRect();
    const px = (event.clientX - rect.left) / rect.width;
    const py = (event.clientY - rect.top) / rect.height;
    const factor = Math.exp(event.deltaY * 0.001);
    const nextWidth = Math.min(base[2] * 8, Math.max(base[2] / 80, view[2] * factor));
    const nextHeight = nextWidth * rect.height / rect.width;
    view[0] += (view[2] - nextWidth) * px;
    view[1] += (view[3] - nextHeight) * py;
    view[2] = nextWidth;
    view[3] = nextHeight;
    applyView();
  }, { passive: false });
  svg.addEventListener('pointerdown', (event) => {
    drag = { x: event.clientX, y: event.clientY, view: view.slice() };
    svg.setPointerCapture(event.pointerId);
    svg.classList.add('dragging');
  });
  svg.addEventListener('pointermove', (event) => {
    if (!drag) return;
    const rect = svg.getBoundingClientRect();
    view[0] = drag.view[0] - (event.clientX - drag.x) * drag.view[2] / rect.width;
    view[1] = drag.view[1] - (event.clientY - drag.y) * drag.view[3] / rect.height;
    applyView();
  });
  const stopDrag = () => { drag = null; svg.classList.remove('dragging'); };
  svg.addEventListener('pointerup', stopDrag);
  svg.addEventListener('pointercancel', stopDrag);
})();
</script>
</body>
</html>
"#,
    );
    Ok(html)
}

fn render_routes(
    html: &mut String,
    report: &IntegratedLayoutReport,
    transport: TransportKind,
    layer_id: &str,
    route_class: &str,
) {
    writeln!(html, "      <g id=\"{layer_id}\">").expect("writing to String cannot fail");
    for (index, route) in report.routes.iter().enumerate() {
        if route.transport != transport || route.cells.is_empty() {
            continue;
        }
        let points = route
            .cells
            .iter()
            .map(|cell| format!("{:.1},{:.1}", cell.x as f64 + 0.5, cell.y as f64 + 0.5))
            .collect::<Vec<_>>()
            .join(" ");
        let title = xml_escape(&format!(
            "route {index} | {} | {} cells | {} -> {}",
            route.item,
            route.cells.len(),
            endpoint_name(&route.source),
            endpoint_name(&route.target),
        ));
        writeln!(
            html,
            "        <polyline class=\"route {route_class}\" points=\"{points}\"><title>{title}</title></polyline>"
        )
        .expect("writing to String cannot fail");
        let endpoint_class = match transport {
            TransportKind::Belt => "endpoint-belt",
            TransportKind::Pipe => "endpoint-pipe",
        };
        for (endpoint, cell) in [
            (
                &route.source,
                route.cells.first().expect("route is non-empty"),
            ),
            (
                &route.target,
                route.cells.last().expect("route is non-empty"),
            ),
        ] {
            let boundary_class = if matches!(endpoint, IntegratedRouteEndpoint::Boundary { .. }) {
                " boundary"
            } else {
                ""
            };
            writeln!(
                html,
                "        <circle class=\"endpoint {endpoint_class}{boundary_class}\" cx=\"{:.1}\" cy=\"{:.1}\" r=\".24\"><title>{}</title></circle>",
                cell.x as f64 + 0.5,
                cell.y as f64 + 0.5,
                xml_escape(&endpoint_name(endpoint)),
            )
            .expect("writing to String cannot fail");
        }
    }
    html.push_str("      </g>\n");
}

fn render_components(html: &mut String, components: &[PlacedLogisticsComponent]) {
    html.push_str("      <g id=\"component-layer\">\n");
    for component in components {
        let class = match component.kind {
            LogisticsComponentKind::Bridge => "bridge",
            LogisticsComponentKind::Splitter => "splitter",
            LogisticsComponentKind::Converger => "converger",
        };
        writeln!(
            html,
            "        <circle class=\"component {class}\" cx=\"{:.1}\" cy=\"{:.1}\" r=\".34\"><title>{}</title></circle>",
            component.position.x as f64 + 0.5,
            component.position.y as f64 + 0.5,
            xml_escape(&format!(
                "{} | {} | {:?}",
                component.id, component.component, component.transport
            )),
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n");
}

fn render_facilities(html: &mut String, report: &IntegratedLayoutReport) {
    html.push_str("      <g id=\"facility-layer\">\n");
    for (index, placement) in report.placements.iter().enumerate() {
        let title = xml_escape(&format!(
            "F{index:02} | {} | {} | {} | ({}, {}) {}x{} r{}",
            placement.facility,
            placement.recipe,
            placement.instance,
            placement.x,
            placement.y,
            placement.width,
            placement.height,
            placement.rotation,
        ));
        writeln!(
            html,
            "        <rect class=\"facility\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><title>{title}</title></rect>",
            placement.x, placement.y, placement.width, placement.height,
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n      <g id=\"label-layer\">\n");
    for (index, placement) in report.placements.iter().enumerate() {
        writeln!(
            html,
            "        <text class=\"facility-label\" x=\"{:.2}\" y=\"{:.2}\">F{index:02}</text>",
            placement.x as f64 + placement.width as f64 / 2.0,
            placement.y as f64 + placement.height as f64 / 2.0,
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n");
}

fn endpoint_name(endpoint: &IntegratedRouteEndpoint) -> String {
    match endpoint {
        IntegratedRouteEndpoint::Facility { instance, port } => {
            format!("facility {instance} port {port}")
        }
        IntegratedRouteEndpoint::Boundary { node, side } => {
            format!("boundary {node} {side:?}")
        }
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use crate::layouts::{
        BoundarySide, FacilityPlacement, FacilityPlacementBounds, IntegratedLayoutDiagnostic,
        IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint,
        WorldGridPosition,
    };
    use crate::logistics::TransportKind;
    use crate::recipes::Rate;

    use super::render_integrated_layout_html;

    #[test]
    fn renders_a_self_contained_wireframe_with_transport_layers() {
        let report = IntegratedLayoutReport {
            success: true,
            status: IntegratedLayoutStatus::Feasible,
            bounds: Some(FacilityPlacementBounds {
                width: 8,
                height: 6,
            }),
            placements: vec![FacilityPlacement {
                instance: "facility:<one>".to_string(),
                recipe: "recipe&one".to_string(),
                facility: "assembler".to_string(),
                x: 2,
                y: 2,
                width: 2,
                height: 2,
                rotation: 0,
            }],
            logistics_components: Vec::new(),
            routes: vec![IntegratedRoute {
                source: IntegratedRouteEndpoint::Boundary {
                    node: "external".to_string(),
                    side: BoundarySide::West,
                },
                target: IntegratedRouteEndpoint::Facility {
                    instance: "facility:<one>".to_string(),
                    port: "input".to_string(),
                },
                item: "item&one".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                transport: TransportKind::Belt,
                cells: vec![
                    WorldGridPosition { x: 0, y: 3 },
                    WorldGridPosition { x: 1, y: 3 },
                ],
            }],
            diagnostics: Vec::new(),
        };

        let html = render_integrated_layout_html(&report).expect("wireframe should render");

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("id=\"belt-layer\""));
        assert!(html.contains("id=\"pipe-layer\""));
        assert!(html.contains("facility:&lt;one&gt;"));
        assert!(html.contains("recipe&amp;one"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn rejects_a_failed_layout_report() {
        let report = IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
            "test", "/", None, "test",
        ));

        let diagnostic = render_integrated_layout_html(&report)
            .expect_err("failed layout should not render a misleading wireframe");

        assert_eq!(diagnostic.code, "layout-visualization-requires-success");
    }
}
