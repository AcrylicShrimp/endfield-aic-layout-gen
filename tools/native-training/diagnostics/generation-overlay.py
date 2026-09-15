"""Create an isolated Rust source copy with generation-only wall phase timers."""
import argparse
from pathlib import Path
import shutil


PROFILE = r'''
struct GenerationProfile {
    slot: u64,
    began: Instant,
    since: Instant,
    start_us: u128,
    phase: &'static str,
    phases: BTreeMap<&'static str, f64>,
    success: bool,
}
impl GenerationProfile {
    fn new(slot: u64) -> Self {
        Self { slot, began: Instant::now(), since: Instant::now(),
            start_us: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros(),
            phase: "draw", phases: BTreeMap::new(), success: false }
    }
    fn phase(&mut self, next: &'static str) {
        *self.phases.entry(self.phase).or_default() += self.since.elapsed().as_secs_f64();
        self.phase = next;
        self.since = Instant::now();
    }
}
impl Drop for GenerationProfile {
    fn drop(&mut self) {
        self.phase("finished");
        eprintln!("{}", json!({"schema":"aic-generation-phase-profile-v1",
            "slot":self.slot,"thread":format!("{:?}",std::thread::current().id()),
            "start_us":self.start_us,"seconds":self.began.elapsed().as_secs_f64(),
            "success":self.success,"phases":self.phases}));
    }
}
'''


def create(source, output):
    if output.exists():
        raise ValueError(f'Output already exists: {output}')
    original = source / 'crates/aic-pilot-native/src/full_factory/generation/source.rs'
    code = original.read_text()
    def replace(before, after):
        nonlocal code
        if code.count(before) != 1:
            raise ValueError(f'Expected one source anchor: {before!r}')
        code = code.replace(before, after)
    replace('        let started = Instant::now();',
            '        let started = Instant::now();\n        let mut profile = GenerationProfile::new(slot);')
    for anchor, phase in [
        ('            let wiring = match process(', 'flow_process'),
        ('            let (request, target, utilizations) = request_from_wiring(&wiring)?;', 'wiring_conversion'),
        ('            let report = match process(', 'construction_process'),
        ('            let node = &report["final_node"];', 'witness_conversion'),
        ('            let randomized = match randomize(', 'randomization'),
        ('            if self.reserved.contains(&randomized.geometry_sha256) {', 'reservation'),
        ('            let metadata = json!', 'metadata'),
        ('            let input = directory.join("input.json");', 'publication'),
    ]:
        replace(anchor, f'            profile.phase("{phase}");\n{anchor}')
    replace('            return Ok(value);\n        }\n        write(',
            '            profile.success = true;\n            return Ok(value);\n        }\n        write(')
    replace('    let mut child = Command::new(binary)',
            '    let mut child = Command::new(binary)')
    replace('    let stdout = child.stdout.take().unwrap();',
            '    let spawn_seconds = start.elapsed().as_secs_f64();\n    let stdout = child.stdout.take().unwrap();')
    replace('    let status = status?;', '''    let status = status?;
    let child_profiles: Vec<serde_json::Value> = String::from_utf8_lossy(&errors)
        .lines().filter_map(|line| line.strip_prefix("AIC_CHILD_PROFILE "))
        .filter_map(|line| serde_json::from_str(line).ok()).collect();
    eprintln!("{}", json!({"schema":"aic-generation-process-profile-v1",
        "command":args.get(1),"request":args.iter().position(|a|a=="--request" || a=="--source-plan").and_then(|i|args.get(i+1)),
        "seconds":start.elapsed().as_secs_f64(),"spawn_seconds":spawn_seconds,
        "child":child_profiles,"success":status.success()}));''')
    output.mkdir(parents=True)
    for name in ('Cargo.toml', 'Cargo.lock'):
        shutil.copy2(source / name, output / name)
    shutil.copytree(source / 'crates', output / 'crates')
    (output / original.relative_to(source)).write_text(code + PROFILE)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    create(args.source.resolve(), args.output.resolve())
