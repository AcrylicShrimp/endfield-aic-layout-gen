"""External protocol admission must not weaken source or input hash verification."""

import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest

from audit import verify_source_hashes


class AuditInputTests(unittest.TestCase):
    def test_explicit_external_input_and_source_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            source = root / "source.py"
            source.write_text("original")
            subprocess.run(["git", "add", "source.py"], cwd=root, check=True)
            subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
                            "commit", "-qm", "Source"], cwd=root, check=True)
            external = root / "protocol.json"
            external.write_text("{}")
            digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
            recorded = {"source.py": digest(source), "protocol.json": digest(external)}
            with self.assertRaisesRegex(ValueError, "audit.source_missing"):
                verify_source_hashes(root, "HEAD", recorded)
            verify_source_hashes(root, "HEAD", recorded, [external])
            external.write_text('{"changed":true}')
            with self.assertRaisesRegex(ValueError, "differ"):
                verify_source_hashes(root, "HEAD", recorded, [external])
            source.write_text("tampered")
            with self.assertRaisesRegex(ValueError, "differ"):
                verify_source_hashes(root, "HEAD", {"source.py": digest(source)}, [source])
