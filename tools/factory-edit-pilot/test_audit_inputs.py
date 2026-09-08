"""External protocol admission must not weaken source or input hash verification."""

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from audit import verify_source_hashes
from case_curriculum_audit import verify_case_provenance


class AuditInputTests(unittest.TestCase):
    def test_case_provenance_resolves_commit_and_rejects_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.com",
                            "commit", "--allow-empty", "-qm", "Source"], cwd=root, check=True)
            commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            case = root / "A"
            case.mkdir()
            provenance = {"git_head": commit, "git_worktree_clean": True}
            metadata = {"provenance": {"A": provenance}}
            saved = {"provenance": provenance, "config": {}, "training_config": {}}
            path = case / "metadata.json"
            path.write_text(json.dumps(saved))
            for revision in (commit, commit[:7], "HEAD"):
                self.assertEqual(verify_case_provenance(root, root, revision, metadata, ["A"], {}, {}), commit)
            with self.assertRaisesRegex(ValueError, "case_curriculum_audit.source"):
                verify_case_provenance(root, root, "missing-commit", metadata, ["A"], {}, {})
            for field, value in (("git_worktree_clean", False), ("git_head", "0"*40)):
                original = provenance[field]
                provenance[field] = value
                path.write_text(json.dumps(saved))
                with self.assertRaisesRegex(ValueError, "case_curriculum_audit.provenance"):
                    verify_case_provenance(root, root, commit, metadata, ["A"], {}, {})
                provenance[field] = original

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
