"""Offline documentation must remain complete, reproducible and installable."""

import ast
import importlib.util
import io
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "plugin_docs", Path(__file__).with_name("export-plugin-docs.py")
)
assert SPEC is not None and SPEC.loader is not None
docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(docs)


class PluginDocsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.derived = json.loads(
            docs.run(
                "cargo",
                "run",
                "--quiet",
                "--locked",
                "-p",
                "smabar-core",
                "--example",
                "export_plugin_reference",
            )
        )
        cls.output = docs.build("https://smabar.com", cls.derived)

    def test_contracts_and_every_class_survive_the_export(self):
        for filename, source in [
            ("guide.json", "plugin-guide/guide.json"),
            ("ui-kit.json", "ui-kit/contract.json"),
        ]:
            expected = json.loads((docs.APP / source).read_text())
            if filename == "ui-kit.json":
                expected["classes"] += json.loads(
                    (docs.APP / "ui-kit/kit-classes.json").read_text()
                )["classes"]
            self.assertEqual(json.loads(self.output[docs.ASSETS / filename]), expected)
        self.assertEqual(
            json.loads(self.output[docs.ASSETS / "theme.json"]), self.derived["theme"]
        )
        metadata = json.loads(self.output[docs.ASSETS / "export.json"])
        for prefix, source in [
            ("guide", "guide.json"),
            ("ui", "ui-kit.json"),
            ("theme", "theme.json"),
        ]:
            for key in json.loads(self.output[docs.ASSETS / source]):
                target = metadata["coverage"][f"{prefix}/{key}"]
                self.assertIn(docs.DOCS / Path(target).name, self.output)

    def test_complete_zips_have_the_original_files_and_valid_manifests(self):
        metadata = json.loads(self.output[docs.ASSETS / "export.json"])
        bundled = {
            Path(path).parent.name
            for path in docs.run(
                "git", "ls-files", "--", "plugins/*/smabar.json"
            ).splitlines()
        }
        self.assertEqual(set(metadata["examples"]) - {"template"}, bundled)
        with tempfile.TemporaryDirectory() as temporary:
            plugin_dirs = []
            for name, example in metadata["examples"].items():
                expected = docs.source_files(example["source"])
                expected["LICENSE"] = (docs.APP / example["license"]).read_bytes()
                data = self.output[docs.ASSETS / f"examples/{name}.zip"]
                self.assertEqual(data, docs.archive(example["id"], expected))
                with zipfile.ZipFile(io.BytesIO(data)) as archive:
                    self.assertIsNone(archive.testzip())
                    self.assertEqual(
                        set(archive.namelist()),
                        {f"{example['id']}/{p}" for p in expected},
                    )
                    for path, content in expected.items():
                        if path.endswith(".py"):
                            ast.parse(content, filename=path)
                        self.assertEqual(
                            archive.read(f"{example['id']}/{path}"), content
                        )
                        download = f"{path}.txt" if path == "LICENSE" else path
                        self.assertEqual(
                            self.output[docs.ASSETS / f"examples/{name}/{download}"],
                            content,
                        )
                    # These are our own just-generated archives, never untrusted input.
                    archive.extractall(Path(temporary) / name)
                plugin_dirs.append(str(Path(temporary) / name / example["id"]))
            docs.run(
                "cargo",
                "run",
                "--quiet",
                "--locked",
                "-p",
                "smabar-core",
                "--example",
                "export_plugin_reference",
                "--",
                *plugin_dirs,
            )

    def test_check_detects_drift_without_writing_and_sync_removes_only_owned_files(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            website = Path(temporary)
            self.assertTrue(docs.sync(website, self.output, check=False))
            self.assertTrue(docs.sync(website, self.output, check=True))
            owned = website / docs.DOCS / "plugin-ref-guide-sdk.md"
            owned.write_text("changed")
            unrelated = website / docs.DOCS / "install.md"
            unrelated.write_text("keep")
            obsolete = website / docs.DOCS / "plugin-ref-obsolete.md"
            obsolete.write_text("obsolete")
            self.assertFalse(docs.sync(website, self.output, check=True))
            self.assertEqual(owned.read_text(), "changed")
            docs.sync(website, self.output, check=False)
            self.assertFalse(obsolete.exists())
            self.assertEqual(unrelated.read_text(), "keep")
            self.assertTrue(docs.sync(website, self.output, check=True))

    def test_markdown_fences_preserve_nested_examples(self):
        sample = "# Readme\n```python\nprint('hello')\n```\n"
        self.assertEqual(
            docs.fence(sample, "markdown"), f"````markdown\n{sample}````\n"
        )


if __name__ == "__main__":
    unittest.main()
