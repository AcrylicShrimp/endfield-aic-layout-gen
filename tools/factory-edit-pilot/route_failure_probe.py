#!/usr/bin/env python3
"""Replay the retained September 10 route failure in an isolated checkout."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise RuntimeError(f"Diagnostic source anchor is absent or ambiguous: {old}")
    return text.replace(old, new)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--checkout", required=True, type=Path)
    parser.add_argument("--build-dir", type=Path)
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    source = args.input.resolve()
    output = args.output.resolve()
    checkout = args.checkout.resolve()
    for name in ("request.json", "failure-before.json", "failure-action.json"):
        if not (source / name).is_file():
            parser.error(f"Missing retained input: {source / name}")
    if checkout.exists() or output.exists():
        parser.error("Checkout and output must be new paths; existing evidence is preserved")
    subprocess.run(
        ["git", "worktree", "add", "--detach", str(checkout), "410aaef"],
        cwd=root, check=True,
    )
    module = checkout / "crates/aic-pilot-native/src/full_factory"
    shutil.copyfile(Path(__file__).with_suffix(".rs"), module / "tests/failure_probe.rs")
    tests = module / "tests.rs"
    tests.write_text(tests.read_text() + "\npub(crate) mod failure_probe;\n")
    routing = module / "routing.rs"
    text = routing.read_text()
    probe = "super::tests::failure_probe"
    text = replace_once(text, "const CROSSING_LIMIT: usize = 2;",
                        f"let crossing_limit = {probe}::crossings();")
    text = replace_once(text, "let bounds = extent(&original)?;", f"""let mut bounds = extent(&original)?;
    let halo = {probe}::halo();
    bounds.min_x -= halo; bounds.min_y -= halo;
    bounds.max_x += halo; bounds.max_y += halo;""")
    text = replace_once(text,
        "jobs.sort_by_key(|j| (1 - layer(j.chain.transport), j.chain.id));",
        f"jobs.sort_by_key(|j| (1 - layer(j.chain.transport), {probe}::priority(j.chain.id), j.chain.id));")
    text = text.replace("CROSSING_LIMIT", "crossing_limit")
    text = replace_once(text, "crossing_limit: crossing_limit - installed,",
        f"crossing_limit: (crossing_limit - installed).min({probe}::per_chain()),")
    text = replace_once(text,
        "let sites = path::crossing_options(&carriers, c, catalogs, &excluded);",
        f"""let mut sites = path::crossing_options(&carriers, c, catalogs, &excluded);
        if {probe}::ban_gate() {{ sites.retain(|(p, _), _| *p != Point {{ x: 15, y: 11 }}); }}""")
    text = replace_once(text, "        expansions += result.expanded;", f"""        {probe}::capture(serde_json::json!({{
            "chain": c, "start": job.start, "end": job.end, "bounds": bounds,
            "blocked": blocked.iter().collect::<Vec<_>>(), "body": body.iter().collect::<Vec<_>>(),
            "occupied": occupied[tr].iter().collect::<Vec<_>>(),
            "reservations": reservations[tr].iter().map(|(p, ids)| serde_json::json!({{"point": p, "chains": ids.iter().collect::<Vec<_>>()}})).collect::<Vec<_>>(),
            "crossing_sites": sites.values().collect::<Vec<_>>(), "crossing_limit": crossing_limit-installed,
            "result": result, "changed": changed.iter().collect::<Vec<_>>(),
            "paths": paths.iter().collect::<Vec<_>>(), "witness": w
        }}));
        expansions += result.expanded;""")
    routing.write_text(text)
    if args.prepare_only:
        print(f"Prepared diagnostic checkout: {checkout}")
        return
    env = os.environ.copy()
    env["AIC_ROUTE_PROBE_INPUT"] = str(source)
    env["AIC_ROUTE_PROBE_OUTPUT"] = str(output)
    env["CARGO_TARGET_DIR"] = str((args.build_dir or checkout / "target").resolve())
    subprocess.run([
        "cargo", "test", "--release", "-p", "aic-pilot-native",
        "retained_route_failure_probe", "--", "--ignored", "--nocapture",
    ], cwd=checkout, env=env, check=True)
    print(f"Diagnostic results: {output / 'summary.json'}")


if __name__ == "__main__":
    main()
