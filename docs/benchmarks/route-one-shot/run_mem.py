import resource
import subprocess

result = subprocess.run(
    ["target/release/examples/one_shot_probe"],
    check=True,
    capture_output=True,
    text=True,
)
print(result.stdout, end="")
print(f"max_rss_kib={resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss}")
