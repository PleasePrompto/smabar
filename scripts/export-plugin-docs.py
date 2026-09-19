#!/usr/bin/env python3
"""Export the app's authoring contracts into the standalone website checkout."""

import argparse
import hashlib
import io
import json
import re
import subprocess
import sys
import zipfile
from collections import defaultdict
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
DOCS = Path("src/content/docs/en")
ASSETS = Path("public/plugin-reference")
EXAMPLES = {
    "template": "Start here: folder watcher, background work, cached state and cover selection.",
    "clock": "Clock, dates, solar calculations and a multi-file flyout.",
    "systeminfo": "System providers, resource metrics and progress indicators.",
    "weather": "Network requests, location settings, cached forecasts and error states.",
    "crypto": "Network prices, charts and a rotating cover.",
    "media": "Media providers, artwork, playback actions and volume.",
    "todos": "Forms, persistence, multiple modules, reminders, sound and tests.",
}
# Keep reference reads small enough for browser tools; split at records, never truncate.
PAGE_BYTES = 24_000


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=APP, text=True)


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()


def slug(text: str) -> str:
    return re.sub(
        r"[^a-z0-9]+", "-", re.sub(r"([a-z])([A-Z])", r"\1-\2", text).lower()
    ).strip("-")


def fence(text: str, language: str = "text") -> str:
    # A README or code sample may itself contain fenced Markdown.
    width = max((len(m[0]) + 1 for m in re.finditer(r"`+", text)), default=3)
    delimiter = "`" * max(3, width)
    return f"{delimiter}{language}\n{text.rstrip()}\n{delimiter}\n"


def records(value: object) -> str:
    """Keep every field verbatim, including markup that must not render as HTML."""
    if not isinstance(value, dict):
        return fence(json.dumps(value, indent=2, ensure_ascii=False), "json")
    return "\n".join(
        f"## {key}\n\n" + fence(json.dumps(item, indent=2, ensure_ascii=False), "json")
        for key, item in value.items()
    )


def archive(plugin_id: str, files: dict[str, bytes]) -> bytes:
    result = io.BytesIO()
    with zipfile.ZipFile(result, "w", compression=zipfile.ZIP_DEFLATED) as output:
        for name, data in sorted(files.items()):
            entry = zipfile.ZipInfo(f"{plugin_id}/{name}")
            entry.create_system = 3
            entry.external_attr = 0o100644 << 16
            output.writestr(entry, data, compress_type=zipfile.ZIP_DEFLATED)
    return result.getvalue()


def source_files(directory: str) -> dict[str, bytes]:
    # Only versioned example files: no local settings, secrets or interpreter caches.
    paths = run("git", "ls-files", "-z", "--", directory).split("\0")
    files = {}
    for name in filter(None, paths):
        path = APP / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Expected a regular tracked file: {name}")
        files[path.relative_to(APP / directory).as_posix()] = path.read_bytes()
    if not files:
        raise ValueError(f"No tracked files in {directory}")
    return files


def build(site: str, derived: dict) -> dict[Path, bytes]:
    site = site.rstrip("/")
    inputs = [
        "plugin-guide",
        "ui-kit",
        "themes",
        "plugins",
        "sdk/python/smabar_sdk",
        "crates/smabar-core/src/plugins/manifest.rs",
        "crates/smabar-core/src/themes",
        "crates/smabar-core/src/config",
        "Cargo.toml",
    ]
    revision, date = (
        run("git", "log", "-1", "--format=%H %cs", "--", *inputs).strip().split()
    )
    guide = json.loads((APP / "plugin-guide/guide.json").read_text())
    kit = json.loads((APP / "ui-kit/contract.json").read_text())
    kit["classes"] += json.loads((APP / "ui-kit/kit-classes.json").read_text())[
        "classes"
    ]
    theme = derived["theme"]
    if (
        not isinstance(theme, dict)
        or not theme.get("themeSchema")
        or not theme.get("referenceThemes")
    ):
        raise ValueError("Rust theme export is incomplete; check ui-kit contracts")
    output: dict[Path, bytes] = {}
    coverage: dict[str, str] = {}

    def link(name: str, label: str | None = None) -> str:
        return f"[{label or name}]({site}/docs/plugin-ref-{slug(name)}.md)"

    def asset(name: str) -> str:
        return f"{site}/plugin-reference/{name}"

    def page(name: str, title: str, body: str, *, index: bool = False) -> str:
        path = DOCS / f"plugin-ref-{slug(name)}.md"
        if path in output:
            raise ValueError(f"Duplicate documentation path: {path}")
        header = (
            f"---\ntitle: {json.dumps(title)}\n"
            f"description: {json.dumps('smabar plugin reference: ' + title)}\n"
            f"order: {30 + len(output)}\nupdated: {date}\nreference: {str(not index).lower()}\n"
            "generated: true\n---\n\n"
            f"[Build my plugin]({site}/build-my-plugin.md) · {link('index', 'All references')} · "
            f"[Install the ZIP]({site}/docs/install-plugin-zip.md)\n\n"
            f"Source: smabar {derived['version']}, revision `{revision}`. "
            f"[Export metadata and checksums]({asset('export.json')}).\n\n"
            "This is an offline reference. MCP calls quoted below describe the connected workflow; "
            "use the linked documents and create local files instead. Live state, installed plugins, "
            "available providers and the user's active theme are unknown. Never claim a live check.\n\n"
        )
        output[path] = (header + body.rstrip() + "\n").encode()
        return link(name, title)

    def section_pages(prefix: str, data: dict) -> list[str]:
        links = []
        for key, value in data.items():
            name = f"{prefix}-{key}"
            title = f"{prefix.title()}: {key}"
            links.append(reference(name, title, value))
            coverage[f"{prefix}/{key}"] = f"/docs/plugin-ref-{slug(name)}.md"
        return links

    def reference(name: str, title: str, value: object) -> str:
        related = f"Read with {link('styling')} and {link('examples')}.\n\n"
        if len(json_bytes(value)) <= PAGE_BYTES or not isinstance(value, (dict, list)):
            return page(name, title, related + records(value))
        entries = (
            list(value.items()) if isinstance(value, dict) else list(enumerate(value))
        )
        chunks = []
        chunk = []
        size = 0
        for key, item in entries:
            entry_size = len(json_bytes({str(key): item}))
            if chunk and size + entry_size > PAGE_BYTES:
                chunks.append(chunk)
                chunk, size = [], 0
            chunk.append((key, item))
            size += entry_size
        if chunk:
            chunks.append(chunk)
        children = []
        for number, chunk in enumerate(chunks, 1):
            label = f"{chunk[0][0]} … {chunk[-1][0]}"
            if len(chunk) == 1:
                label, part = chunk[0]
            else:
                part = (
                    dict(chunk)
                    if isinstance(value, dict)
                    else [item for _, item in chunk]
                )
            child = reference(f"{name}-part-{number}", f"{title}: {label}", part)
            children.append(f"- {child}")
        return page(
            name,
            title,
            related
            + "Complete reference, split by field or entry:\n\n"
            + "\n".join(children),
        )

    guide_links = section_pages("guide", guide)
    schema_link = reference(
        "manifest-schema", "Manifest JSON Schema", derived["manifestSchema"]
    )
    output[ASSETS / "manifest.schema.json"] = json_bytes(derived["manifestSchema"])
    output[ASSETS / "guide.json"] = json_bytes(guide)
    page(
        "guide",
        "Plugin authoring",
        "\n".join(f"- {x}" for x in [*guide_links, schema_link]),
        index=True,
    )

    categories = defaultdict(list)
    for entry in kit["classes"]:
        categories[entry["category"]].append(entry)
    class_links = []
    for category, entries in sorted(categories.items()):
        related = (
            "\n\nRead "
            + ", ".join(
                link(f"ui-{key}", key)
                for key in [
                    "designRules",
                    "tileSizing",
                    "sanitizer",
                    "behaviour",
                    "conventions",
                    "coverLayouts",
                    "charts",
                    "icons",
                    "media",
                    "formContract",
                ]
            )
            + " before using these classes.\n\n"
        )
        class_links.append(
            page(
                f"classes-{category}",
                f"Classes: {category}",
                related + records(entries),
            )
        )
    page("ui-classes", "UI class categories", "\n".join(f"- {x}" for x in class_links))
    coverage["ui/classes"] = "/docs/plugin-ref-ui-classes.md"
    ui_links = section_pages("ui", {k: v for k, v in kit.items() if k != "classes"})
    ui_links.append(link("ui-classes", "Every core and component class, by category"))
    output[ASSETS / "ui-kit.json"] = json_bytes(kit)
    page(
        "styling",
        "Styling and interaction",
        (
            "Read designRules, bestPractices, tileSizing, coverLayouts and sanitizer before writing HTML. "
            "Then read the classes for your UI, the full behaviour hooks and relevant snippets. "
            "All sections below are complete; coverLayouts and behaviour are not shortened indexes.\n\n"
            + "\n".join(f"- {x}" for x in ui_links)
            + f"\n\nTheme values and their definitions: {link('tokens')}. "
            + f"[Complete UI contract as JSON]({asset('ui-kit.json')})."
        ),
        index=True,
    )

    token_links = []
    for key, value in theme.items():
        if key in {"baseTokens", "publicComponentTokens", "internalTokens"}:
            groups = defaultdict(dict)
            for token, metadata in value.items():
                groups[metadata.get("group", "other")][token] = metadata
            children = [
                reference(f"tokens-{key}-{group}", f"{key}: {group}", items)
                for group, items in sorted(groups.items())
            ]
            body = (
                "\n".join(f"- {child}" for child in children)
                or "No entries in this version."
            )
        name = f"tokens-{key}"
        token_links.append(
            page(name, f"Theme contract: {key}", body)
            if key in {"baseTokens", "publicComponentTokens", "internalTokens"}
            else reference(name, f"Theme contract: {key}", value)
        )
        coverage[f"theme/{key}"] = f"/docs/plugin-ref-{slug(name)}.md"
    output[ASSETS / "theme.json"] = json_bytes(theme)
    page(
        "tokens",
        "Theme and token reference",
        (
            "Reference themes are bundled defaults, not the user's active values. Use documented "
            "classes and var(--sb-*) tokens; do not copy a reference palette into plugin markup. "
            "Internal tokens are implementation metadata, not a public styling API.\n\n"
            + "\n".join(f"- {x}" for x in token_links)
            + f"\n\n[Complete theme contract as JSON]({asset('theme.json')})."
        ),
        index=True,
    )

    example_links = []
    examples = {}
    for name, purpose in EXAMPLES.items():
        directory = "plugin-guide/template" if name == "template" else f"plugins/{name}"
        files = source_files(directory)
        license_path = "LICENSE" if name == "template" else "plugins/LICENSE"
        files["LICENSE"] = (APP / license_path).read_bytes()
        plugin_id = json.loads(files["smabar.json"])["id"]
        zip_name = f"examples/{name}.zip"
        output[ASSETS / zip_name] = archive(plugin_id, files)
        examples[name] = {
            "id": plugin_id,
            "source": directory,
            "files": sorted(files),
            "license": license_path,
            "archive": asset(zip_name),
        }
        body = (
            f"{purpose}\n\n[Complete ZIP]({asset(zip_name)}). "
            + f"Read {link('styling')}, {link('guide-sdk', 'SDK')}, "
            + f"{link('guide-manifest', 'manifest')}, {link('ui-conventions', 'conventions')}, "
            + f"{link('ui-behaviour', 'behaviour')}, {link('ui-coverLayouts', 'covers')} "
            + f"and {link('ui-classes', 'classes')} alongside this example.\n\n"
            + "The template retains the app's PolyForm Shield license; bundled plugins are MIT. "
            + "Keep the included license when reusing substantial source. "
            + "Bundled plugin IDs already exist in smabar: choose a new ID for your own plugin.\n\n"
            + "## Files\n\n"
            + fence("\n".join(f"{plugin_id}/{path}" for path in sorted(files)))
        )
        for path, data in sorted(files.items()):
            # Astro treats extensionless URLs as page paths and adds a slash.
            download_path = path if Path(path).suffix else f"{path}.txt"
            target = f"examples/{name}/{download_path}"
            output[ASSETS / target] = data
            if (
                Path(path).suffix in {".py", ".json", ".md", ".txt"}
                or path == "LICENSE"
            ):
                language = {".py": "python", ".json": "json", ".md": "markdown"}.get(
                    Path(path).suffix, "text"
                )
                file_page = page(
                    f"example-{name}-{path}",
                    f"{name}: {path}",
                    f"Part of {link(f'example-{name}', name)}. Read every source file linked there.\n\n"
                    + f"[Download original file]({asset(target)}).\n\n"
                    + fence(data.decode("utf-8"), language),
                )
                body += f"\n- {file_page}\n"
            else:
                body += f"\n- [{path}]({asset(target)}): binary asset, {len(data)} bytes, included in the ZIP.\n"
        example_links.append(page(f"example-{name}", f"Example: {name}", body))
    page(
        "examples",
        "Complete example plugins",
        "\n".join(
            f"- {entry}: {purpose}"
            for entry, purpose in zip(example_links, EXAMPLES.values(), strict=True)
        ),
        index=True,
    )
    page(
        "index",
        "Plugin reference index",
        (
            "Choose a topic; follow the linked Markdown chapters as needed. "
            "The build workflow and deliverable checklist live in the entry document.\n\n"
            + "\n".join(
                f"- {link(name, title)}"
                for name, title in [
                    (
                        "guide",
                        "Authoring: manifest, SDK, providers, storage and lifecycle",
                    ),
                    (
                        "styling",
                        "Styling: design, classes, covers, hooks, forms, icons and charts",
                    ),
                    (
                        "tokens",
                        "Tokens: definitions, schemas and bundled reference themes",
                    ),
                    ("examples", "Complete template and bundled plugins"),
                ]
            )
        ),
    )
    output[ASSETS / "export.json"] = json_bytes(
        {
            "version": derived["version"],
            "sourceRevision": revision,
            "sourceDate": date,
            "coverage": coverage,
            "examples": examples,
            "files": {
                path.as_posix(): hashlib.sha256(data).hexdigest()
                for path, data in sorted(
                    output.items(), key=lambda item: item[0].as_posix()
                )
            },
        }
    )
    return output


def sync(website: Path, output: dict[Path, bytes], check: bool) -> bool:
    existing = {
        p.relative_to(website) for p in (website / DOCS).glob("plugin-ref-*.md")
    }
    existing.update(
        p.relative_to(website) for p in (website / ASSETS).rglob("*") if p.is_file()
    )
    changed = [
        p
        for p, data in output.items()
        if not (website / p).is_file() or (website / p).read_bytes() != data
    ]
    removed = existing - output.keys()
    if check:
        if changed or removed:
            print(
                "Plugin documentation is stale; run the export again:", file=sys.stderr
            )
            for path in sorted([*changed, *removed]):
                print(f"  {path}", file=sys.stderr)
        return not (changed or removed)
    for path in changed:
        target = website / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(output[path])
    for path in removed:
        (website / path).unlink()
    print(
        f"Plugin docs: {len(changed)} written, {len(removed)} obsolete files removed."
    )
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("website", type=Path, help="Standalone website checkout")
    parser.add_argument(
        "--check", action="store_true", help="Fail on stale output without writing"
    )
    parser.add_argument(
        "--site", default="https://smabar.com", help="Canonical documentation origin"
    )
    args = parser.parse_args()
    if not (args.website / "src/content/docs/en").is_dir():
        parser.error("website must contain src/content/docs/en")
    try:
        derived = json.loads(
            run(
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
        return (
            0
            if sync(args.website.resolve(), build(args.site, derived), args.check)
            else 1
        )
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Plugin documentation export failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
