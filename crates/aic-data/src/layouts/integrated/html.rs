use std::collections::BTreeSet;
use std::fmt::Write;

use crate::localization::ValidatedLocalizationCatalog;
use crate::logistics::{LogisticsComponentKind, TransportKind};

use super::{
    FacilityPlacement, FacilityPlacementBounds, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    PlacedLogisticsComponent, TransportNetwork, TransportNetworkEndpoint,
};

pub(crate) struct LayoutVisualizationPage<'a> {
    pub(crate) bounds: &'a FacilityPlacementBounds,
    pub(crate) placements: &'a [FacilityPlacement],
    pub(crate) logistics_components: &'a [PlacedLogisticsComponent],
    pub(crate) transport_networks: &'a [TransportNetwork],
    pub(crate) introduced_facilities: BTreeSet<&'a str>,
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) history: bool,
}

pub fn render_integrated_layout_html(
    report: &IntegratedLayoutReport,
) -> Result<String, IntegratedLayoutDiagnostic> {
    render_integrated_layout_html_with_localization(report, None)
}

pub fn render_integrated_layout_html_with_localization(
    report: &IntegratedLayoutReport,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String, IntegratedLayoutDiagnostic> {
    let has_direct_geometry = report.bounds.is_some()
        && (!report.placements.is_empty()
            || !report.transport_networks.is_empty()
            || !report.logistics_components.is_empty());
    if !report.success && report.phases.is_empty() && !has_direct_geometry {
        return Ok(render_failure_summary(report));
    }
    let pages = collect_pages(report)?;
    let run_status = match (report.success, report.status, report.phases.is_empty()) {
        (true, super::IntegratedLayoutStatus::Optimal, _) => {
            "<span class=\"run-status success\">OPTIMAL</span>"
        }
        (true, _, _) => "<span class=\"run-status success\">FEASIBLE</span>",
        (false, _, true) => {
            "<span class=\"run-status failure\">REJECTED · INVALID INCUMBENT</span>"
        }
        (false, _, false) => "<span class=\"run-status failure\">FAILED · PARTIAL HISTORY</span>",
    };
    render_layout_visualization_pages(&pages, run_status, !report.success, localization)
}

pub(crate) fn render_layout_history_html(
    pages: &[LayoutVisualizationPage<'_>],
    success: bool,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String, IntegratedLayoutDiagnostic> {
    if pages.is_empty() {
        return Err(IntegratedLayoutDiagnostic::error(
            "layout-visualization-missing-pages",
            "/phases",
            None,
            "layout history visualization requires at least one geometry page",
        ));
    }
    let run_status = if success {
        "<span class=\"run-status success\">CONSTRUCTED</span>"
    } else {
        "<span class=\"run-status failure\">FAILED · PARTIAL HISTORY</span>"
    };
    render_layout_visualization_pages(pages, run_status, !success, localization)
}

fn render_layout_visualization_pages(
    pages: &[LayoutVisualizationPage<'_>],
    run_status: &str,
    partial_final: bool,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String, IntegratedLayoutDiagnostic> {
    let final_page = pages.last().expect("validated pages are non-empty");
    let width = final_page.bounds.width.max(1);
    let height = final_page.bounds.height.max(1);
    let total_route_cells = pages
        .iter()
        .map(|page| {
            page.transport_networks
                .iter()
                .map(|network| network.cells.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let final_metrics = page_metrics(final_page);
    let mut html = String::with_capacity(total_route_cells.saturating_mul(10).max(32_768));
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
  .run-status {{ border: 1px solid currentColor; padding: 3px 6px; font-size: 11px; }}
  .run-status.success {{ color: #6ff2bd; }}
  .run-status.failure {{ color: #ff6b9c; }}
  .metrics {{ color: #8ba8bd; font-size: 12px; margin-right: auto; }}
  .phase-nav {{ display: inline-flex; align-items: center; gap: 7px; }}
  .phase-label {{ min-width: 92px; color: #cbe7f8; font-size: 12px; text-align: center; }}
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
  .route-cell {{ shape-rendering: crispEdges; }}
  .route-cell-belt {{ fill: #f1b84b; fill-opacity: .58; stroke: #ffd77c; stroke-width: .05; }}
  .route-cell-pipe {{ fill: #59c7f2; fill-opacity: .62; stroke: #b3ebff; stroke-width: .06; }}
  .route-direction {{ fill: #071019; stroke: #ffffff; stroke-width: .7px; vector-effect: non-scaling-stroke; pointer-events: none; }}
  [data-inspect] {{ cursor: pointer; }}
  .route-group.selected .route-cell {{ fill-opacity: .95; stroke: #ffffff; stroke-width: .12; }}
  .endpoint.selected, .facility.selected, .component.selected {{ stroke: #ffffff; stroke-width: 2px; }}
  .endpoint {{ vector-effect: non-scaling-stroke; stroke-width: 1px; }}
  .endpoint-belt {{ fill: #f1b84b; stroke: #251a07; }}
  .endpoint-pipe {{ fill: #59c7f2; stroke: #071c26; }}
  .port-input {{ stroke: #6ff2bd; stroke-width: 1.6px; stroke-linejoin: round; }}
  .port-output {{ stroke: #ff6b9c; stroke-width: 1.6px; stroke-linejoin: round; }}
  .external {{ fill: #071019; stroke: #ffec99; stroke-width: 1.5px; vector-effect: non-scaling-stroke; }}
  .facility {{ fill: #10293a; fill-opacity: .92; stroke: #d7efff; stroke-width: 1.25px; vector-effect: non-scaling-stroke; }}
  .facility.introduced {{ fill: #163c34; stroke: #6ff2bd; stroke-width: 2px; }}
  .facility:hover {{ fill: #19415a; stroke: #ffffff; stroke-width: 2.5px; }}
  .facility-label {{ fill: #dff3ff; font-size: .72px; text-anchor: middle; dominant-baseline: central; pointer-events: none; }}
  .facility-index {{ fill: #7ea6bb; font-size: .62px; }}
  .component {{ fill: #071019; vector-effect: non-scaling-stroke; }}
  .bridge {{ stroke: #ff6b85; stroke-width: 1.5px; }}
  .splitter {{ stroke: #c4f06f; stroke-width: 1.5px; }}
  .converger {{ stroke: #d99cff; stroke-width: 1.5px; }}
  .hidden-layer {{ display: none; }}
  .hidden-phase {{ display: none; }}
  .help {{ position: absolute; right: 10px; bottom: 8px; color: #6f8b9e; font-size: 11px; pointer-events: none; }}
  .inspect-panel {{ position: absolute; top: 12px; right: 12px; width: min(420px, calc(100% - 24px)); max-height: calc(100% - 48px); overflow: auto; border: 1px solid #5f8198; background: rgba(7, 16, 25, .96); box-shadow: 0 12px 36px rgba(0, 0, 0, .45); padding: 12px; z-index: 3; }}
  .inspect-panel[hidden] {{ display: none; }}
  .inspect-header {{ display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 9px; color: #f2f8ff; }}
  .inspect-content {{ margin: 0; color: #bfe6fb; font: inherit; font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }}
</style>
</head>
<body>
<div id="aic-layout-viewer">
  <div class="toolbar">
    <span class="title">AIC LAYOUT WIREFRAME</span>
    {}
    <span class="phase-nav"><button type="button" data-previous>Previous</button><span class="phase-label" data-phase-label></span><button type="button" data-next>Next</button></span>
    <span class="metrics" data-metrics>{}</span>
    <button type="button" data-toggle="belt-layer" aria-pressed="true"><span class="swatch belt-swatch"></span>Belt (<span data-belt-summary>{}</span>)</button>
    <button type="button" data-toggle="pipe-layer" aria-pressed="true"><span class="swatch pipe-swatch"></span>Pipe (<span data-pipe-summary>{}</span>)</button>
    <span title="Arrows follow material flow"><span style="color:#6ff2bd">➤ IN</span> · <span style="color:#ff6b9c">➤ OUT</span></span>
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
"##,
        run_status,
        final_metrics,
        transport_summary(final_page, TransportKind::Belt),
        transport_summary(final_page, TransportKind::Pipe),
        width + 4,
        height + 4,
        width + 4,
        height + 4,
    )
    .expect("writing to String cannot fail");

    for (index, page) in pages.iter().enumerate() {
        render_page(
            &mut html,
            page,
            index,
            pages.len(),
            partial_final && index + 1 == pages.len(),
            localization,
        );
    }

    html.push_str(
        r#"    </svg>
    <aside class="inspect-panel" data-inspector hidden><div class="inspect-header"><strong>Layout details</strong><button type="button" data-inspector-close>Close</button></div><pre class="inspect-content" data-inspector-content></pre></aside>
    <div class="help">wheel: zoom · drag: pan · click: inspect</div>
  </div>
</div>
<script>
(() => {
  const root = document.getElementById('aic-layout-viewer');
  const svg = root.querySelector('svg');
  const pages = Array.from(root.querySelectorAll('.phase-page'));
  let pageIndex = pages.length - 1;
  let base = pages[pageIndex].dataset.baseView.split(' ').map(Number);
  let view = base.slice();
  let drag = null;
  let pinned = null;
  const inspector = root.querySelector('[data-inspector]');
  const inspectorContent = root.querySelector('[data-inspector-content]');
  const inspectTarget = (target) => target instanceof Element ? target.closest('[data-inspect]') : null;
  const showDetails = (target) => {
    if (!target) { inspector.hidden = true; inspectorContent.textContent = ''; return; }
    inspector.hidden = false;
    inspectorContent.textContent = target.dataset.inspect.split(' | ').join('\n');
  };
  const selectDetails = (target) => {
    if (pinned) pinned.classList.remove('selected');
    pinned = target;
    if (pinned) pinned.classList.add('selected');
    showDetails(pinned);
  };
  const applyView = () => svg.setAttribute('viewBox', view.join(' '));
  const showPage = (nextIndex) => {
    pageIndex = Math.max(0, Math.min(pages.length - 1, nextIndex));
    pages.forEach((page, index) => page.classList.toggle('hidden-phase', index !== pageIndex));
    const page = pages[pageIndex];
    base = page.dataset.baseView.split(' ').map(Number);
    view = base.slice();
    applyView();
    root.querySelector('[data-phase-label]').textContent = page.dataset.phaseLabel;
    root.querySelector('[data-metrics]').textContent = page.dataset.metrics;
    root.querySelector('[data-belt-summary]').textContent = page.dataset.beltSummary;
    root.querySelector('[data-pipe-summary]').textContent = page.dataset.pipeSummary;
    root.querySelector('[data-previous]').disabled = pageIndex === 0;
    root.querySelector('[data-next]').disabled = pageIndex + 1 === pages.length;
  };
  root.querySelector('[data-previous]').addEventListener('click', () => showPage(pageIndex - 1));
  root.querySelector('[data-next]').addEventListener('click', () => showPage(pageIndex + 1));
  root.querySelectorAll('[data-toggle]').forEach((button) => {
    button.addEventListener('click', () => {
      const pressed = button.getAttribute('aria-pressed') === 'true';
      button.setAttribute('aria-pressed', String(!pressed));
      root.querySelectorAll(`.${button.dataset.toggle}`).forEach((layer) => layer.classList.toggle('hidden-layer', pressed));
    });
  });
  root.querySelector('[data-reset]').addEventListener('click', () => { view = base.slice(); applyView(); });
  root.querySelector('[data-inspector-close]').addEventListener('click', () => selectDetails(null));
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
    drag = { x: event.clientX, y: event.clientY, view: view.slice(), target: inspectTarget(event.target), moved: false };
    svg.setPointerCapture(event.pointerId);
    svg.classList.add('dragging');
  });
  svg.addEventListener('pointermove', (event) => {
    if (!drag) return;
    if (Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < 5) return;
    drag.moved = true;
    const rect = svg.getBoundingClientRect();
    view[0] = drag.view[0] - (event.clientX - drag.x) * drag.view[2] / rect.width;
    view[1] = drag.view[1] - (event.clientY - drag.y) * drag.view[3] / rect.height;
    applyView();
  });
  const stopDrag = (cancelled) => {
    if (drag && !drag.moved && !cancelled) selectDetails(drag.target);
    drag = null;
    svg.classList.remove('dragging');
  };
  svg.addEventListener('pointerup', () => stopDrag(false));
  svg.addEventListener('pointercancel', () => stopDrag(true));
  showPage(pageIndex);
})();
</script>
</body>
</html>
"#,
    );
    Ok(html)
}

fn render_failure_summary(report: &IntegratedLayoutReport) -> String {
    let exact = report.exact.as_ref().map_or_else(
        || "No solver metrics were produced.".to_string(),
        |exact| {
            format!(
                "Formulation: {}\nFacilities: {}\nLogical requirements: {}\nTransport networks: {}\nNetwork terminals: {}\nMaximum flow scale: {}\nMaximum line capacity units: {}\nTotal terminal flow units: {}\nNetwork flow variables: {}\nBranch component variables: {}\nGrid cells: {}\nConstruction: {} ms\nSearch: {} ms\nIncumbents: {}\nTermination: {:?}\nProof: {:?}\nValidation: {:?}",
                exact.formulation,
                exact.model.facility_count,
                exact.model.route_requirement_count,
                exact.model.commodity_network_count,
                exact.model.network_terminal_count,
                exact.model.maximum_network_flow_scale,
                exact.model.maximum_line_capacity_units,
                exact.model.total_terminal_flow_units,
                exact.model.network_flow_variables,
                exact.model.branch_component_variables,
                exact.model.grid_cell_count,
                exact.construction_ms,
                exact.search_ms,
                exact.incumbent_count,
                exact.termination,
                exact.proof,
                exact.validation,
            )
        },
    );
    let diagnostics = report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "<li><code>{}</code><p>{}</p></li>",
                xml_escape(diagnostic.code),
                xml_escape(&diagnostic.message),
            )
        })
        .collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>AIC Layout Solve Result</title><style>:root{{color-scheme:dark;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}}body{{margin:0;background:#071019;color:#dbeeff}}main{{max-width:900px;margin:8vh auto;padding:24px}}h1{{color:#ff6b9c}}section{{border:1px solid #35536a;background:#0b1722;padding:18px;margin:16px 0}}pre,p{{color:#bfe6fb;white-space:pre-wrap;line-height:1.55}}code{{color:#ff9bbc}}li{{margin:12px 0}}</style></head><body><main><h1>LAYOUT SOLVE DID NOT PRODUCE GEOMETRY</h1><p>Status: {:?}</p><section><h2>Solver evidence</h2><pre>{}</pre></section><section><h2>Diagnostics</h2><ul>{}</ul></section></main></body></html>",
        report.status,
        xml_escape(&exact),
        diagnostics,
    )
}

fn collect_pages(
    report: &IntegratedLayoutReport,
) -> Result<Vec<LayoutVisualizationPage<'_>>, IntegratedLayoutDiagnostic> {
    let pages = if report.phases.is_empty() {
        let bounds = report.bounds.as_ref().ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "layout-visualization-missing-bounds",
                "/bounds",
                None,
                "successful integrated layout report has no used bounds",
            )
        })?;
        vec![LayoutVisualizationPage {
            bounds,
            placements: &report.placements,
            logistics_components: &report.logistics_components,
            transport_networks: &report.transport_networks,
            introduced_facilities: BTreeSet::new(),
            label: "Final layout".to_string(),
            detail: String::new(),
            history: false,
        }]
    } else {
        report
            .phases
            .iter()
            .map(|phase| {
                let objective = phase.exact.objective;
                LayoutVisualizationPage {
                    bounds: &phase.bounds,
                    placements: &phase.placements,
                    logistics_components: &phase.logistics_components,
                    transport_networks: &phase.transport_networks,
                    introduced_facilities: phase
                        .introduced_facilities
                        .iter()
                        .map(String::as_str)
                        .collect(),
                    label: if phase.introduced_components.is_empty() {
                        "Final refinement".to_string()
                    } else {
                        format!("Phase {}/{}", phase.index + 1, report.phases.len())
                    },
                    detail: format!(
                        " · +{} facilities · {} warm-start variables · area {} · {} transport tiles · {} turns · {} ms search",
                        phase.introduced_facilities.len(),
                        phase.exact.model.hint_variables,
                        objective.map_or(0, |value| value.used_bounding_box_area),
                        objective.map_or(0, |value| value.physical_transport_tiles),
                        objective.map_or(0, |value| value.total_route_turns),
                        phase.exact.search_ms,
                    ),
                    history: true,
                }
            })
            .collect()
    };
    for (index, page) in pages.iter().enumerate() {
        if page.bounds.width < 0 || page.bounds.height < 0 {
            return Err(IntegratedLayoutDiagnostic::error(
                "layout-visualization-invalid-bounds",
                format!("/phases/{index}/bounds"),
                None,
                format!(
                    "layout visualization requires non-negative used bounds, found {} by {}",
                    page.bounds.width, page.bounds.height
                ),
            ));
        }
    }
    Ok(pages)
}

fn render_page(
    html: &mut String,
    page: &LayoutVisualizationPage<'_>,
    index: usize,
    page_count: usize,
    partial_final: bool,
    localization: Option<&ValidatedLocalizationCatalog>,
) {
    let width = page.bounds.width.max(1);
    let height = page.bounds.height.max(1);
    let hidden = if index + 1 == page_count {
        ""
    } else {
        " hidden-phase"
    };
    let mut phase_label = page.label.clone();
    if partial_final {
        phase_label.push_str(if page.history {
            " · last valid"
        } else {
            " · rejected incumbent"
        });
    }
    writeln!(
        html,
        "      <g class=\"phase-page{hidden}\" data-base-view=\"-2 -2 {} {}\" data-phase-label=\"{}\" data-metrics=\"{}\" data-belt-summary=\"{}\" data-pipe-summary=\"{}\">",
        width + 4,
        height + 4,
        xml_escape(&phase_label),
        xml_escape(&page_metrics(page)),
        transport_summary(page, TransportKind::Belt),
        transport_summary(page, TransportKind::Pipe),
    )
    .expect("writing to String cannot fail");
    writeln!(
        html,
        "        <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"url(#major-grid)\" stroke=\"#4a6a80\" stroke-width=\".2\"/>",
        page.bounds.width, page.bounds.height,
    )
    .expect("writing to String cannot fail");
    render_transport_networks(
        html,
        page.transport_networks,
        page.placements,
        TransportKind::Belt,
        "route-cell-belt",
        localization,
    );
    render_transport_networks(
        html,
        page.transport_networks,
        page.placements,
        TransportKind::Pipe,
        "route-cell-pipe",
        localization,
    );
    render_components(html, page.logistics_components);
    render_facilities(
        html,
        page.placements,
        &page.introduced_facilities,
        localization,
    );
    html.push_str("      </g>\n");
}

fn page_metrics(page: &LayoutVisualizationPage<'_>) -> String {
    let route_cells = transport_cell_count(page, None);
    let belt_cells = transport_cell_count(page, Some(TransportKind::Belt));
    let pipe_cells = transport_cell_count(page, Some(TransportKind::Pipe));
    let bridge_count = page
        .logistics_components
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Bridge)
        .count();
    format!(
        "{}×{} · {} facilities · {} occupied transport tiles (belt {} / pipe {}) · {} bridges{}",
        page.bounds.width,
        page.bounds.height,
        page.placements.len(),
        route_cells,
        belt_cells,
        pipe_cells,
        bridge_count,
        page.detail,
    )
}

fn transport_cell_count(
    page: &LayoutVisualizationPage<'_>,
    transport: Option<TransportKind>,
) -> usize {
    page.transport_networks
        .iter()
        .filter(|network| transport.is_none_or(|transport| network.transport == transport))
        .map(|network| network.cells.len())
        .sum::<usize>()
}

fn transport_summary(page: &LayoutVisualizationPage<'_>, transport: TransportKind) -> String {
    let tiles = transport_cell_count(page, Some(transport));
    format!("{tiles} tiles")
}

fn render_transport_networks(
    html: &mut String,
    networks: &[TransportNetwork],
    placements: &[FacilityPlacement],
    transport: TransportKind,
    route_class: &str,
    localization: Option<&ValidatedLocalizationCatalog>,
) {
    let layer_class = match transport {
        TransportKind::Belt => "belt-layer",
        TransportKind::Pipe => "pipe-layer",
    };
    writeln!(html, "        <g class=\"{layer_class}\">").expect("writing to String cannot fail");
    for network in networks {
        if network.transport != transport || network.cells.is_empty() {
            continue;
        }
        let item_name = localized_item_name(localization, &network.item);
        let title = xml_escape(&format!(
            "{:?} transport network | {} | item {} | {} terminals | {} occupied tiles | {} logical requirements",
            network.transport,
            network.id,
            item_name,
            network.terminals.len(),
            network.cells.len(),
            network.requirement_ids.len(),
        ));
        writeln!(
            html,
            "          <g class=\"route-group\" data-inspect=\"{title}\">"
        )
        .expect("writing to String cannot fail");
        for cell in &network.cells {
            writeln!(
                html,
                "            <rect class=\"route-cell {route_class}\" x=\"{}\" y=\"{}\" width=\"1\" height=\"1\"/>",
                cell.x, cell.y,
            )
            .expect("writing to String cannot fail");
        }
        for (segment_index, segment) in network.segments.iter().enumerate() {
            if network.segments.len() > 8 && segment_index % 8 != 4 {
                continue;
            }
            let points = flow_arrow_points(&segment.from, &segment.from, &segment.to, 0.34, 0.22);
            writeln!(
                html,
                "            <polygon class=\"route-direction\" points=\"{points}\"/>"
            )
            .expect("writing to String cannot fail");
        }
        html.push_str("          </g>\n");
        let endpoint_class = match transport {
            TransportKind::Belt => "endpoint-belt",
            TransportKind::Pipe => "endpoint-pipe",
        };
        for terminal in &network.terminals {
            let is_source = terminal.direction == crate::facilities::FacilityPortDirection::Output;
            let endpoint = &terminal.endpoint;
            let cell = &terminal.position;
            let external_class = if matches!(endpoint, TransportNetworkEndpoint::External { .. }) {
                " external"
            } else {
                ""
            };
            let role = endpoint_role(endpoint, is_source);
            let tooltip = xml_escape(&format!(
                "{role} | item {} | rate {} | {} | network {}",
                item_name,
                rate_label(terminal.rate),
                endpoint_name(endpoint),
                network.id,
            ));
            let neighboring = if is_source {
                network
                    .segments
                    .iter()
                    .find(|segment| segment.from == *cell)
                    .map(|segment| &segment.to)
            } else {
                network
                    .segments
                    .iter()
                    .find(|segment| segment.to == *cell)
                    .map(|segment| &segment.from)
            }
            .unwrap_or(cell);
            let (dx, dy) = endpoint_arrow_direction(
                endpoint,
                is_source,
                cell,
                if is_source { cell } else { neighboring },
                if is_source { neighboring } else { cell },
                placements,
            );
            let marker_cell =
                facility_port_cell(endpoint, cell, placements).unwrap_or_else(|| cell.clone());
            let points = arrow_points_in_direction(&marker_cell, dx, dy, 0.48, 0.36);
            let role_class = if is_source {
                "port-output"
            } else {
                "port-input"
            };
            writeln!(
                html,
                "          <polygon class=\"endpoint {role_class} {endpoint_class}{external_class}\" data-inspect=\"{tooltip}\" points=\"{points}\"/>"
            )
            .expect("writing to String cannot fail");
        }
    }
    html.push_str("        </g>\n");
}

fn facility_port_cell(
    endpoint: &TransportNetworkEndpoint,
    connection: &super::WorldGridPosition,
    placements: &[FacilityPlacement],
) -> Option<super::WorldGridPosition> {
    let TransportNetworkEndpoint::Facility { instance, .. } = endpoint else {
        return None;
    };
    let placement = placements
        .iter()
        .find(|placement| placement.instance == instance.as_str())?;
    let inside_horizontal = placement.x..(placement.x + placement.width);
    let inside_vertical = placement.y..(placement.y + placement.height);
    if connection.x == placement.x - 1 && inside_vertical.contains(&connection.y) {
        Some(super::WorldGridPosition {
            x: placement.x,
            y: connection.y,
        })
    } else if connection.x == placement.x + placement.width
        && inside_vertical.contains(&connection.y)
    {
        Some(super::WorldGridPosition {
            x: placement.x + placement.width - 1,
            y: connection.y,
        })
    } else if connection.y == placement.y - 1 && inside_horizontal.contains(&connection.x) {
        Some(super::WorldGridPosition {
            x: connection.x,
            y: placement.y,
        })
    } else if connection.y == placement.y + placement.height
        && inside_horizontal.contains(&connection.x)
    {
        Some(super::WorldGridPosition {
            x: connection.x,
            y: placement.y + placement.height - 1,
        })
    } else {
        None
    }
}

fn flow_arrow_points(
    center: &super::WorldGridPosition,
    from: &super::WorldGridPosition,
    to: &super::WorldGridPosition,
    forward: f64,
    half_width: f64,
) -> String {
    let dx = (to.x - from.x).signum() as f64;
    let dy = (to.y - from.y).signum() as f64;
    let (dx, dy) = if dx == 0.0 && dy == 0.0 {
        (1.0, 0.0)
    } else {
        (dx, dy)
    };
    arrow_points_in_direction(center, dx, dy, forward, half_width)
}

fn arrow_points_in_direction(
    center: &super::WorldGridPosition,
    dx: f64,
    dy: f64,
    forward: f64,
    half_width: f64,
) -> String {
    let center_x = center.x as f64 + 0.5;
    let center_y = center.y as f64 + 0.5;
    let tip_x = center_x + dx * forward;
    let tip_y = center_y + dy * forward;
    let base_x = center_x - dx * forward * 0.7;
    let base_y = center_y - dy * forward * 0.7;
    let perpendicular_x = -dy * half_width;
    let perpendicular_y = dx * half_width;
    format!(
        "{tip_x:.2},{tip_y:.2} {:.2},{:.2} {:.2},{:.2}",
        base_x + perpendicular_x,
        base_y + perpendicular_y,
        base_x - perpendicular_x,
        base_y - perpendicular_y,
    )
}

fn endpoint_arrow_direction(
    endpoint: &TransportNetworkEndpoint,
    is_source: bool,
    cell: &super::WorldGridPosition,
    route_from: &super::WorldGridPosition,
    route_to: &super::WorldGridPosition,
    placements: &[FacilityPlacement],
) -> (f64, f64) {
    if let TransportNetworkEndpoint::External { side, .. } = endpoint {
        let (dx, dy) = match side {
            crate::facilities::FacilityPortEdge::North => (0.0, -1.0),
            crate::facilities::FacilityPortEdge::East => (1.0, 0.0),
            crate::facilities::FacilityPortEdge::South => (0.0, 1.0),
            crate::facilities::FacilityPortEdge::West => (-1.0, 0.0),
        };
        return if is_source { (-dx, -dy) } else { (dx, dy) };
    }
    if let TransportNetworkEndpoint::Facility { instance, .. } = endpoint
        && let Some(placement) = placements
            .iter()
            .find(|placement| placement.instance == instance.as_str())
    {
        let outward = if cell.x < placement.x {
            Some((-1.0, 0.0))
        } else if cell.x >= placement.x + placement.width {
            Some((1.0, 0.0))
        } else if cell.y < placement.y {
            Some((0.0, -1.0))
        } else if cell.y >= placement.y + placement.height {
            Some((0.0, 1.0))
        } else {
            None
        };
        if let Some((dx, dy)) = outward {
            return if is_source { (dx, dy) } else { (-dx, -dy) };
        }
    }

    let dx = (route_to.x - route_from.x).signum() as f64;
    let dy = (route_to.y - route_from.y).signum() as f64;
    if dx == 0.0 && dy == 0.0 {
        (1.0, 0.0)
    } else {
        (dx, dy)
    }
}

fn endpoint_role(endpoint: &TransportNetworkEndpoint, is_source: bool) -> &'static str {
    match (endpoint, is_source) {
        (TransportNetworkEndpoint::Facility { .. }, true) => "facility output port",
        (TransportNetworkEndpoint::Facility { .. }, false) => "facility input port",
        (TransportNetworkEndpoint::External { .. }, true) => "factory external input",
        (TransportNetworkEndpoint::External { .. }, false) => "factory external output",
    }
}

fn rate_label(rate: crate::recipes::Rate) -> String {
    if rate.denominator == 1 {
        format!("{}/s", rate.numerator)
    } else {
        format!(
            "{}/{} per second ({:.3}/s)",
            rate.numerator,
            rate.denominator,
            rate.numerator as f64 / rate.denominator as f64,
        )
    }
}

fn render_components(html: &mut String, components: &[PlacedLogisticsComponent]) {
    html.push_str("      <g class=\"component-layer\">\n");
    for component in components {
        let class = match component.kind {
            LogisticsComponentKind::Bridge => "bridge",
            LogisticsComponentKind::Splitter => "splitter",
            LogisticsComponentKind::Converger => "converger",
        };
        writeln!(
            html,
            "        <circle class=\"component {class}\" data-inspect=\"{}\" cx=\"{:.1}\" cy=\"{:.1}\" r=\".34\"/>",
            xml_escape(&format!(
                "logistics component | {} | {} | {:?}",
                component.id, component.component, component.transport
            )),
            component.position.x as f64 + 0.5,
            component.position.y as f64 + 0.5,
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n");
}

fn render_facilities(
    html: &mut String,
    placements: &[FacilityPlacement],
    introduced_facilities: &BTreeSet<&str>,
    localization: Option<&ValidatedLocalizationCatalog>,
) {
    html.push_str("      <g class=\"facility-layer\">\n");
    for (index, placement) in placements.iter().enumerate() {
        let localized = localization.and_then(|catalog| catalog.facility(&placement.facility));
        let facility_name = localized.map_or(placement.facility.as_str(), |entry| {
            entry.facility_name.as_str()
        });
        let mode_name = localized.map_or("unknown mode", |entry| entry.mode_name.as_str());
        let title = xml_escape(&format!(
            "F{index:02} | {} | {} | mode {} | recipe {} | {} | ({}, {}) {}x{} r{}",
            facility_name,
            placement.facility,
            mode_name,
            placement.recipe,
            placement.instance,
            placement.x,
            placement.y,
            placement.width,
            placement.height,
            placement.rotation,
        ));
        let introduced = if introduced_facilities.contains(placement.instance.as_str()) {
            " introduced"
        } else {
            ""
        };
        writeln!(
            html,
            "        <rect class=\"facility{introduced}\" data-inspect=\"{title}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            placement.x, placement.y, placement.width, placement.height,
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n      <g class=\"label-layer\">\n");
    for (index, placement) in placements.iter().enumerate() {
        let facility_name = localization
            .and_then(|catalog| catalog.facility(&placement.facility))
            .map_or(placement.facility.as_str(), |entry| {
                entry.facility_name.as_str()
            });
        let center_x = placement.x as f64 + placement.width as f64 / 2.0;
        let center_y = placement.y as f64 + placement.height as f64 / 2.0;
        let available_width = (placement.width as f64 - 0.6).max(0.5);
        let label_font_size = fitted_label_font_size(facility_name, available_width);
        writeln!(
            html,
            "        <text class=\"facility-label\" x=\"{center_x:.2}\" y=\"{:.2}\"><tspan x=\"{center_x:.2}\" style=\"font-size:{label_font_size:.3}px\">{}</tspan><tspan class=\"facility-index\" x=\"{center_x:.2}\" dy=\"1\">F{index:02}</tspan></text>",
            center_y - 0.45,
            xml_escape(facility_name),
        )
        .expect("writing to String cannot fail");
    }
    html.push_str("      </g>\n");
}

fn estimated_label_width(value: &str) -> f64 {
    value
        .chars()
        .map(|character| if character.is_ascii() { 0.43 } else { 0.72 })
        .sum()
}

fn fitted_label_font_size(value: &str, available_width: f64) -> f64 {
    const BASE_FONT_SIZE: f64 = 0.72;
    let estimated_width = estimated_label_width(value);
    if estimated_width <= available_width || estimated_width == 0.0 {
        BASE_FONT_SIZE
    } else {
        BASE_FONT_SIZE * available_width / estimated_width
    }
}

fn localized_item_name(localization: Option<&ValidatedLocalizationCatalog>, item: &str) -> String {
    localization
        .and_then(|catalog| catalog.item(item))
        .map_or_else(
            || item.to_string(),
            |entry| format!("{} ({item})", entry.display_name),
        )
}

fn endpoint_name(endpoint: &TransportNetworkEndpoint) -> String {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, port } => {
            format!("facility {instance} port {port}")
        }
        TransportNetworkEndpoint::External { node, side } => {
            format!("external {node} beside {side:?} facility port")
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
    use crate::facilities::{FacilityPortDirection, FacilityPortEdge};
    use crate::layouts::{
        ExactModelMetrics, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
        ExactValidationStatus, FacilityPlacement, FacilityPlacementBounds,
        INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic, IntegratedLayoutPhase,
        IntegratedLayoutReport, IntegratedLayoutStatus, TransportNetwork, TransportNetworkEndpoint,
        TransportNetworkSegment, TransportNetworkTerminal, WorldGridPosition,
    };
    use crate::localization::{
        LocalizationCatalog, LocalizationTextSource, LocalizedFacility, LocalizedName,
        ValidatedLocalizationCatalog,
    };
    use crate::logistics::TransportKind;
    use crate::recipes::Rate;

    use super::{
        endpoint_arrow_direction, estimated_label_width, facility_port_cell,
        fitted_label_font_size, render_integrated_layout_html,
        render_integrated_layout_html_with_localization,
    };

    #[test]
    fn renders_a_self_contained_wireframe_with_transport_layers() {
        let mut report = IntegratedLayoutReport {
            schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
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
            transport_networks: vec![TransportNetwork {
                id: "network:belt:item&one".to_string(),
                requirement_ids: vec!["wiring-edge:test:lane:0000".to_string()],
                item: "item&one".to_string(),
                transport: TransportKind::Belt,
                cells: vec![
                    WorldGridPosition { x: 0, y: 3 },
                    WorldGridPosition { x: 1, y: 3 },
                ],
                segments: vec![TransportNetworkSegment {
                    from: WorldGridPosition { x: 0, y: 3 },
                    to: WorldGridPosition { x: 1, y: 3 },
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                }],
                terminals: vec![
                    TransportNetworkTerminal {
                        id: "network:belt:item&one:terminal:0000".to_string(),
                        node: "external".to_string(),
                        direction: FacilityPortDirection::Output,
                        endpoint: TransportNetworkEndpoint::External {
                            node: "external".to_string(),
                            side: FacilityPortEdge::West,
                        },
                        position: WorldGridPosition { x: 0, y: 3 },
                        rate: Rate {
                            numerator: 1,
                            denominator: 1,
                        },
                    },
                    TransportNetworkTerminal {
                        id: "network:belt:item&one:terminal:0001".to_string(),
                        node: "facility:<one>".to_string(),
                        direction: FacilityPortDirection::Input,
                        endpoint: TransportNetworkEndpoint::Facility {
                            instance: "facility:<one>".to_string(),
                            port: "input".to_string(),
                        },
                        position: WorldGridPosition { x: 1, y: 3 },
                        rate: Rate {
                            numerator: 1,
                            denominator: 1,
                        },
                    },
                ],
                component_ids: Vec::new(),
            }],
            phases: Vec::new(),
            exact: None,
            diagnostics: Vec::new(),
        };
        let phase = |index| IntegratedLayoutPhase {
            index,
            introduced_components: vec![format!("component:{index:04}")],
            introduced_facilities: vec!["facility:<one>".to_string()],
            cumulative_facility_count: 1,
            cumulative_route_requirement_count: 1,
            bounds: report.bounds.clone().expect("test report has bounds"),
            placements: report.placements.clone(),
            logistics_components: report.logistics_components.clone(),
            transport_networks: report.transport_networks.clone(),
            exact: ExactSolveReport {
                formulation: "test",
                model: ExactModelMetrics {
                    hint_variables: usize::from(index > 0),
                    ..ExactModelMetrics::default()
                },
                model_complexity: crate::research::ModelComplexityMetrics::unavailable(),
                construction_ms: 1,
                search_ms: 2,
                first_incumbent_ms: Some(1),
                incumbent_count: 1,
                search_statistics: crate::layouts::integrated::ExactSearchStatistics::default(),
                objective: Some(crate::layouts::ExactObjectiveValue {
                    used_bounding_box_area: 48,
                    physical_transport_tiles: 2,
                    total_route_turns: 0,
                    maximum_used_side: 8,
                    logistics_component_count: 0,
                }),
                objective_stages: Vec::new(),
                termination: ExactTerminationReason::Feasible,
                proof: ExactProofStatus::Unproven,
                validation: ExactValidationStatus::Passed,
            },
        };
        report.phases = vec![phase(0), phase(1)];
        let localization = ValidatedLocalizationCatalog::try_from_catalog(LocalizationCatalog {
            schema_version: 1,
            locale: "ko-KR".to_string(),
            items: Vec::new(),
            facilities: vec![LocalizedFacility {
                id: "assembler".to_string(),
                base_facility: "assembler".to_string(),
                facility_name: "분쇄기".to_string(),
                facility_name_source: LocalizationTextSource::Official,
                mode: "normal".to_string(),
                mode_name: "기본 모드".to_string(),
                mode_name_source: LocalizationTextSource::Official,
            }],
            modes: vec![LocalizedName {
                id: "normal".to_string(),
                display_name: "기본 모드".to_string(),
                display_name_source: LocalizationTextSource::Official,
            }],
            recipe_descriptions: Vec::new(),
        })
        .expect("test localization should validate");

        let html = render_integrated_layout_html_with_localization(&report, Some(&localization))
            .expect("wireframe should render");

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("class=\"belt-layer\""));
        assert!(html.contains("class=\"pipe-layer\""));
        assert!(html.contains("class=\"route-cell route-cell-belt\""));
        assert!(html.contains("width=\"1\" height=\"1\""));
        assert!(html.contains("item item&amp;one | rate 1/s"));
        assert!(html.contains("factory external input"));
        assert!(html.contains("facility input port"));
        assert!(html.contains("port-input endpoint-belt"));
        assert!(html.contains("color:#6ff2bd\">➤ IN"));
        assert!(html.contains("data-inspector-content"));
        assert!(html.contains("data-inspect=\"factory external input"));
        assert!(html.contains("click: inspect"));
        assert!(!html.contains("pointerover"));
        assert!(!html.contains("tabindex=\"0\""));
        assert!(!html.contains("filter: brightness"));
        assert!(html.contains("Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < 5"));
        assert!(html.contains("selectDetails(drag.target)"));
        assert!(html.contains("Belt (<span data-belt-summary>2 tiles</span>)"));
        assert_eq!(html.matches("class=\"phase-page").count(), 2);
        assert!(html.contains("data-previous"));
        assert!(html.contains("Phase 2/2"));
        assert!(html.contains("class=\"facility introduced\""));
        assert!(html.contains("분쇄기</tspan>"));
        assert!(!html.contains("lengthAdjust"));
        assert!(!html.contains("textLength"));
        assert!(html.contains("facility:&lt;one&gt;"));
        assert!(html.contains("recipe&amp;one"));
        assert!(!html.contains("https://"));

        let mut rejected = report.clone();
        rejected.success = false;
        rejected.status = IntegratedLayoutStatus::Unknown;
        rejected.phases.clear();
        rejected.diagnostics.push(IntegratedLayoutDiagnostic::error(
            "invalid-integrated-layout-witness",
            "/transport_networks/0/cells",
            None,
            "the extracted incumbent failed independent validation",
        ));
        let rejected_html = render_integrated_layout_html(&rejected)
            .expect("rejected incumbent geometry should remain inspectable");
        assert!(rejected_html.contains("REJECTED · INVALID INCUMBENT"));
        assert!(rejected_html.contains("Final layout · rejected incumbent"));
        assert!(rejected_html.contains("class=\"route-cell route-cell-belt\""));

        report.success = false;
        report.status = IntegratedLayoutStatus::Unknown;
        report.diagnostics.push(IntegratedLayoutDiagnostic::error(
            "later-phase-failed",
            "/phases/2",
            Some("phase:2".to_string()),
            "the next phase failed",
        ));
        let partial = render_integrated_layout_html(&report)
            .expect("failed layout with completed snapshots should render automatically");
        assert!(partial.contains("FAILED · PARTIAL HISTORY"));
        assert!(partial.contains("Phase 2/2 · last valid"));
        assert_eq!(partial.matches("class=\"phase-page").count(), 2);
    }

    #[test]
    fn renders_solver_evidence_when_failure_has_no_geometry() {
        let report = IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
            "test", "/", None, "test",
        ));

        let html = render_integrated_layout_html(&report)
            .expect("failed layout without geometry should still render solver evidence");

        assert!(html.contains("LAYOUT SOLVE DID NOT PRODUCE GEOMETRY"));
        assert!(html.contains("<code>test</code>"));
    }

    #[test]
    fn preserves_short_korean_facility_label_proportions() {
        assert!(estimated_label_width("천화로") < 4.4);
        assert!(estimated_label_width("xiranite-oven-1-mode-liquid") > 4.4);
        assert_eq!(fitted_label_font_size("천화로", 4.4), 0.72);
        assert!(fitted_label_font_size("xiranite-oven-1-mode-liquid", 4.4) < 0.72);
    }

    #[test]
    fn facility_endpoint_arrows_follow_the_facility_boundary() {
        let placement = FacilityPlacement {
            instance: "facility".to_string(),
            recipe: "recipe".to_string(),
            facility: "assembler".to_string(),
            x: 10,
            y: 10,
            width: 5,
            height: 5,
            rotation: 0,
        };
        let endpoint = TransportNetworkEndpoint::Facility {
            instance: "facility".to_string(),
            port: "port".to_string(),
        };
        let unrelated_route_from = WorldGridPosition { x: 15, y: 11 };
        let unrelated_route_to = WorldGridPosition { x: 15, y: 12 };

        assert_eq!(
            facility_port_cell(
                &endpoint,
                &WorldGridPosition { x: 15, y: 10 },
                std::slice::from_ref(&placement),
            ),
            Some(WorldGridPosition { x: 14, y: 10 }),
            "a corner port keeps its east-facing edge instead of being reinterpreted as north-facing",
        );

        assert_eq!(
            endpoint_arrow_direction(
                &endpoint,
                false,
                &WorldGridPosition { x: 15, y: 12 },
                &unrelated_route_from,
                &unrelated_route_to,
                std::slice::from_ref(&placement),
            ),
            (-1.0, 0.0),
        );
        assert_eq!(
            endpoint_arrow_direction(
                &endpoint,
                true,
                &WorldGridPosition { x: 15, y: 12 },
                &unrelated_route_from,
                &unrelated_route_to,
                std::slice::from_ref(&placement),
            ),
            (1.0, 0.0),
        );
        assert_eq!(
            endpoint_arrow_direction(
                &endpoint,
                false,
                &WorldGridPosition { x: 12, y: 15 },
                &unrelated_route_from,
                &unrelated_route_to,
                &[placement],
            ),
            (0.0, -1.0),
        );
    }
}
