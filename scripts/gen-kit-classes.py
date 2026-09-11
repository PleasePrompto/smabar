#!/usr/bin/env python3
"""Generates the class documentation for the harvested format48 components.

`scripts/harvest-f48.py` brought the component CSS across (shell/src/styles/
kit-components.css). This script brings the DOCUMENTATION across: the same
components' manifests become `ui-kit/kit-classes.json`, which the MCP `ui_kit`
tool serves next to the hand-written entries in `ui-kit/contract.json`. The
same run writes `ui-kit/component-tokens.json`, the component/runtime
custom-property census consumed by the theme contract.

Why a sibling script and a sibling file: harvest-f48.py transforms CSS and its
`--check` means "the stylesheet matches the format48 source". This one reads
the MANIFESTS, verifies against the harvested stylesheet and writes JSON. One
flag per artifact keeps each check unambiguous, and the generated JSON never
touches the hand-written contract.

An entry is only written when the class survives every filter:

1. JavaScript    a component whose behaviour smabar's shell does NOT provide
                 is skipped whole — a plugin emits HTML strings and can never
                 ship a script, so documenting it would be a trap. The ones
                 the shell DOES drive are listed in BEHAVIOUR and documented
                 with the markup hook that activates them.
2. Existence     the class must actually appear in kit-components.css. The
                 harvest dropped rules (sb-section, the sb-toggle family,
                 `position: fixed`, Chrome-only properties), so the manifest
                 is not proof.
3. Cascade       kit-components.css is inside `@layer sb.components`, and
                 un-layered rules beat layered ones whatever the specificity.
                 A class whose own declarations are already declared by an
                 un-layered ui-kit.css rule on the SAME element (itself, or
                 its base for a `--modifier`) therefore does nothing — those
                 are dropped, and smabar's own equivalent stays documented.
4. Duplicates    names already documented by hand in ui-kit/contract.json.

Examples are rebuilt through the sanitizer's real rules (parsed out of
shell/src/plugins/sanitizeRules.ts) so a documented snippet cannot contain
markup the shell would silently remove.

Usage:  scripts/gen-kit-classes.py [--source DIR] [--check | --check-local]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from html.parser import HTMLParser
from importlib import util as import_util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = Path.home() / "Dokumente/Coding/Webprojekt format48 UI"
OUTPUT = ROOT / "ui-kit/kit-classes.json"
TOKENS_OUTPUT = ROOT / "ui-kit/component-tokens.json"
THEME_CONTRACT = ROOT / "ui-kit/theme-contract.json"
CONTRACT = ROOT / "ui-kit/contract.json"
HARVESTED_CSS = ROOT / "shell/src/styles/kit-components.css"
KIT_CSS = ROOT / "shell/src/styles/ui-kit.css"
RULES_TS = ROOT / "shell/src/plugins/sanitizeRules.ts"
CSS_SHEETS = (
    ROOT / "shell/src/styles/globals.css",
    ROOT / "shell/src/styles/bar.css",
    ROOT / "shell/src/styles/overlay.css",
    ROOT / "shell/src/styles/settings.css",
    KIT_CSS,
    HARVESTED_CSS,
)

# Values written by shell behaviour/layout code, or derived only to connect
# two CSS rules. They are implementation state, not supported theme knobs.
INTERNAL_TOKEN = re.compile(
    r"^--sb-(?:"
    r"z-|bar-row-height$|autohide-|shortcut-|work-area-(?:width|height)$|tile-content-h$|"
    r"tile-(?:min|max)-[12]$|magnify-origin$|context-[xy]$|"
    r"range-(?:fill|pct)$|dualrange-(?:from|to)$|"
    r"chart-(?:max$|value$|v[1-4]$|stop-)|card-checked$|"
    r"check-dot-scale$|compare-pos$|drag-marker-width$|field-error-display$|"
    r"indicator-t[xy]$|rating-value$|laser-angle$|draw$|"
    r"mesh-(?:hue|stop)-|lightbox-on-scrim$|probe$"
    r")"
)


def _harvest_module():
    """The harvest script, imported for its token map.

    Both scripts read the same source library, so the mapping from
    `--f48-*` to smabar's tokens must be the SAME one — a second copy here
    would drift the moment a token is remapped. The filename has a hyphen,
    hence the explicit loader.
    """
    path = Path(__file__).with_name("harvest-f48.py")
    spec = import_util.spec_from_file_location("harvest_f48", path)
    if spec is None or spec.loader is None:
        sys.exit(f"cannot load {path}")
    module = import_util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Longest first, so `--f48-surface-2` is not eaten by `--f48-surface`.
HARVEST = _harvest_module()
TOKEN_ORDER = sorted(
    HARVEST.TOKEN_MAP.items(), key=lambda pair: len(pair[0]), reverse=True
)

# The manifests of exactly the components harvest-f48.py took the CSS from.
COMPONENTS = [
    "buttons",
    "forms-text",
    "forms-choice",
    "data",
    "surfaces",
    "content",
    "feedback",
    "disclosure",
    "overlays-modal",
    "overlays-float",
    "layout",
    "indicator",
    "actions",
    "charts",
    "chat",
    "codeblock",
    "command",
    "compare",
    "contextmenu",
    "countdown",
    "datepicker",
    "daterange",
    "dualrange",
    "lightbox",
    "masonry",
    "menubar",
    "motion",
    "multiselect",
    "rating",
    "spoiler",
    "stepper",
    "tablefilter",
    "tablesort",
    "taginput",
    "timeinput",
    "tree",
]

# Components whose behaviour smabar's shell provides (see
# shell/src/plugins/behaviour/). format48 marks them `requiresJs` because the
# LIBRARY ships that script; here the SHELL does, and a plugin activates it
# with markup alone. Each entry names the hook that turns it on, which is what
# an LLM has to know and what the manifest cannot say.
BEHAVIOUR = {
    "copy": "data-sb-copy (or data-sb-copy-text)",
    "number": "data-sb-number",
    "password": "data-sb-password-toggle",
    "range": "data-sb-range",
    "dual-range": "data-sb-dualrange",
    "input-otp": "data-sb-otp",
    "multi-select": "data-sb-multiselect",
    "tag-input": "data-sb-taginput",
    "combobox": "data-sb-combobox",
    "datepicker": "data-sb-datepicker",
    "date-range": "data-sb-daterange",
    "table-sort": "data-sb-sort on each sortable th",
    "table-filter": "data-sb-filter on the search input",
    "countdown": "data-sb-countdown",
    "dropdown": "data-sb-dropdown",
    "menubar": "data-sb-menubar",
    "context-menu": "data-sb-context",
    "command": "data-sb-command",
    "lightbox": "data-sb-lightbox",
    "image-compare": "data-sb-compare",
}

# Two source categories hold a single component each; a section of one is a
# drawer nobody opens. They fold into the closest existing section: a stepper
# reports progress, and the motion utilities decorate content.
CATEGORY_MAP = {"navigation": "feedback", "utilities": "content"}

# Sections of the `ui_kit` tool, one per manifest category.
CATEGORIES = {
    "actions",
    "content",
    "data",
    "disclosure",
    "feedback",
    "forms",
    "layout",
    "overlays",
}

# format48 variants that lose to smabar's un-layered kit (filter 3) but
# appear inside OTHER components' example markup. Rewritten to smabar's own
# equivalent so every example is markup that actually renders as shown.
EXAMPLE_CLASS_MAP = {
    "sb-btn--primary": "sb-btn-primary",
    "sb-btn--ghost": "sb-btn-ghost",
    "sb-btn--danger": "sb-btn-danger",
    "sb-btn--icon": "sb-btn-icon",
    "sb-btn--outline": "",
    "sb-btn--soft": "",
    "sb-btn--sm": "",
    "sb-btn--lg": "",
    "sb-badge--ok": "sb-badge-success",
    "sb-badge--warn": "sb-badge-warning",
    "sb-badge--danger": "sb-badge-danger",
    "sb-badge--info": "sb-badge-info",
    "sb-badge--accent": "sb-badge-accent",
}

# A relative image URL is dropped by the sanitizer; a plugin addresses its own
# files through the asset scheme instead. In-page anchors are dropped too —
# the shell opens a link in the system browser, so only http(s) means anything.
EXAMPLE_SRC_MAP = {"cover.jpg": "sb-asset:cover.jpg"}
EXAMPLE_HREF = "https://example.com"

VOID_TAGS = {"br", "col", "hr", "img", "input", "source", "wbr"}
IMPLICIT_CLOSE = {"dd", "dt", "li", "option", "p", "td", "th", "tr"}

# Elements the HTML parser foster-parents out of the body: a lone <th> simply
# vanishes. A one-element example therefore carries the ancestors it needs.
PARENTS = {
    "caption": ("table",),
    "col": ("table", "colgroup"),
    "colgroup": ("table",),
    "optgroup": ("select",),
    "option": ("select",),
    "tbody": ("table",),
    "td": ("table", "tbody", "tr"),
    "tfoot": ("table",),
    "th": ("table", "thead", "tr"),
    "thead": ("table",),
    "tr": ("table", "tbody"),
}

# Classes the component's own example never shows on the right element, so
# the placement has to be stated rather than derived.
EXAMPLE_OVERRIDES = {
    "is-active": '<button class="sb-btn is-active">Week</button>',
    "sb-chip__close": '<span class="sb-chip">Design<button class="sb-chip__close" '
    'aria-label="Remove" data-action="remove">&times;</button></span>',
    "sb-table__input": '<input class="sb-table__input" value="12" aria-label="Amount">',
    "sb-indicator__dot": '<span class="sb-indicator__dot"></span>',
    "sb-card__media": '<img class="sb-card__media" src="sb-asset:cover.png" alt="">',
    "sb-bento__item--tall": '<div class="sb-card sb-bento__item--tall">…</div>',
    "sb-bento__item--wide": '<div class="sb-card sb-bento__item--wide">…</div>',
}

# Classes of a JS-free component that a script still has to produce. The
# component as a whole is worth documenting; these members are not.
DROP_CLASSES = {
    # format48's themed replacement for the native datalist popup, built by
    # f48.js at runtime. A plugin gets the native popup instead.
    "sb-combobox__list",
    "sb-combobox__option",
}

# The manifests describe a library that ships JavaScript; smabar's plugins
# never do. These rewrites point the affected notes at the HTML-only way.
NOTE_FIXES = [
    (r";? aria-pressed=\"true\" works identically \([^)]*\)", ""),
    # The library's own name means nothing to a plugin author: what drives
    # these components here is the shell's behaviour layer.
    (r"\bf48\.js\b", "the shell"),
    (r"\bJS\b", "the shell"),
    (
        (
            r"Wrap it in \.sb-range-wrap with data-sb-range to get an accent fill "
            r"up to the thumb, min/max labels, and a value bubble on hover/focus "
            r"\(data-sb-range-unit appends a unit like %\)\."
        ),
        (
            "Wrap it in .sb-range-wrap for min/max labels, an accent fill up to "
            'the thumb and a value bubble: set style="--sb-range-fill: 40%; '
            '--sb-range-pct: 0.4" on the wrapper yourself.'
        ),
    ),
    (
        r"wrapper with data-sb-range enabling fill \+ bubble \(f48\.js\)",
        (
            "wrapper carrying the fill and the value bubble; you set "
            "--sb-range-fill (0 to 100%) and --sb-range-pct (0 to 1) on it inline"
        ),
    ),
    (r"; a \.sb-dropdown wrapper \(trigger \+ menu\) works as a segment[^.]*", ""),
    (r"\(e\.g\. on :root, default 50%\)", "(default 50%)"),
    # `datetime` is not on the sanitizer's attribute allowlist.
    (r"use a <time> element with datetime", "use a <time> element"),
    # There is no visually-hidden utility in the kit; `hidden` does the job,
    # and a label still toggles an input it hides.
    (r"\(sb-sr-only\)", "(the hidden attribute)"),
    # The harvest replaced format48's --f48-hue derivation with smabar's
    # own second brand colour.
    (
        r"by a branded --sb-accent plus --sb-hue",
        "by a branded --sb-accent plus --sb-accent-2",
    ),
    # smabar renders its own themed tooltip from `title`; the harvested
    # data-sb-tooltip rules collide with that attribute.
    (r"data-sb-tooltip", "title"),
    # `name` on <details> is not on the attribute allowlist, so exclusive
    # accordions are not available.
    (
        r"; add the same name attribute to make panels mutually exclusive",
        "; every panel opens and closes on its own",
    ),
    # `field-sizing` is one of the Chrome-only properties the harvest drops.
    (
        (
            r"that auto-grows with its content where field-sizing is supported, "
            r"stays vertically resizable everywhere else"
        ),
        "that stays vertically resizable",
    ),
    # WebKitGTK has neither appearance: base-select nor ::picker().
    (r"\. In Chromium 135\+ .*$", "."),
    # <iframe> is dropped by the sanitizer.
    (r"whose direct img, video, or iframe child", "whose direct img or video child"),
]

PURPOSE_OVERRIDES = {
    "sb-btn--sm": "Button: compact size using the shared type and spacing scale",
    "sb-btn--lg": "Button: large size using the shared type and spacing scale",
    "sb-spoiler__toggle": "Spoiler: native <summary> toggle inside the enclosing <details>",
}

# Nothing may reach an agent that points at JavaScript or at a format48-only
# hook — a plugin has neither.
# A purpose must never point at the source library: "f48.js" names something
# a plugin author cannot see. `data-sb-*` on the other hand is exactly what
# they need — and every one named is verified against the behaviour layer by
# check_purpose, so a documented hook that nothing implements fails the build.
NOTE_GUARD = re.compile(r"f48|\bJS\b")

BEHAVIOUR_DIR = ROOT / "shell/src/plugins/behaviour"


def behaviour_hooks() -> set[str]:
    """Every `data-sb-*` attribute a behaviour module actually listens for."""
    hooks: set[str] = set()
    for module in sorted(BEHAVIOUR_DIR.glob("*.ts")):
        if module.name.endswith(".test.ts"):
            continue
        source = module.read_text()
        hooks.update(re.findall(r"\[(data-sb-[a-z-]+)", source))
        # camelCase dataset reads name the same attribute.
        for camel in re.findall(r"dataset\.(sb[A-Z][A-Za-z]*)", source):
            hooks.add("data-" + re.sub(r"(?<!^)([A-Z])", r"-\1", camel).lower())
    return hooks


# --- the shell's rules, parsed from their source -----------------------------


def ts_sets(source: str) -> dict[str, set[str]]:
    """Every `export const NAME = new Set([...])` in sanitizeRules.ts."""
    out: dict[str, set[str]] = {}
    for name, body in re.findall(
        r"export const (\w+) = new Set\(\[(.*?)\]\)", source, re.DOTALL
    ):
        out[name] = set(re.findall(r'"([^"]+)"', body))
    return out


def css_classes(*sheets: str) -> set[str]:
    """Class names any kit stylesheet actually defines."""
    names: set[str] = set()
    for sheet in sheets:
        names.update(re.findall(r"\.((?:sb|is)-[a-zA-Z0-9_-]+)", sheet))
    return names


def strip_comments(css: str) -> str:
    return re.sub(r"/\*.*?\*/", "", css, flags=re.DOTALL)


def blocks(css: str) -> list[tuple[str, str]]:
    """(prelude, body) pairs of a stylesheet, by brace matching."""
    out, depth, start, prelude = [], 0, 0, ""
    for index, char in enumerate(css):
        if char == "{":
            if depth == 0:
                prelude = css[start:index].strip()
                start = index + 1
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                out.append((prelude, css[start:index]))
                start = index + 1
    return out


# A shorthand also settles the longhands a layered rule might set.
SHORTHANDS = {
    "background": {"background-color", "background-image"},
    "border": {"border-color", "border-style", "border-width"},
    "font": {"font-family", "font-size", "font-weight"},
    "margin": {"margin-block", "margin-inline"},
    "padding": {"padding-block", "padding-inline"},
}


def declared_props(body: str) -> set[str]:
    """Property names of a rule body (nested at-rules ignored)."""
    flat = re.sub(r"\{[^{}]*\}", "", body)
    found: set[str] = set()
    for raw in flat.split(";"):
        name = raw.split(":", 1)[0].strip()
        if re.fullmatch(r"(?:--[a-z0-9-]+|[a-z-]+)", name):
            found.add(name)
            found.update(SHORTHANDS.get(name, set()))
    return found


def key_class(selector_part: str) -> str | None:
    """The single class of a bare `.foo` selector part, else None.

    Only single-class, single-compound selectors can be compared safely: they
    match an element by class alone, which is exactly the case where the
    un-layered rule wins over the layered one on the same element.
    """
    part = selector_part.strip()
    if not part or re.search(r"[\s>+~:\[]", part):
        return None
    classes = re.findall(r"^\.([a-zA-Z0-9_-]+)$", part)
    return classes[0] if classes else None


def unlayered_props(css: str) -> dict[str, set[str]]:
    """Properties ui-kit.css sets on an element via a bare class selector."""
    out: dict[str, set[str]] = {}
    for prelude, body in blocks(strip_comments(css)):
        if prelude.startswith("@"):
            continue
        for part in prelude.split(","):
            name = key_class(part)
            if name is not None:
                out.setdefault(name, set()).update(declared_props(body))
    return out


def layered_props(css: str) -> dict[str, set[str]]:
    """Properties kit-components.css sets on an element via a bare class."""
    out: dict[str, set[str]] = {}
    inner = "".join(body for prelude, body in blocks(strip_comments(css)))
    for prelude, body in blocks(inner):
        if prelude.startswith("@"):
            continue
        for part in prelude.split(","):
            name = key_class(part)
            if name is not None:
                out.setdefault(name, set()).update(declared_props(body))
    return out


def base_class(name: str) -> str:
    """`sb-btn--primary` -> `sb-btn`; a `__element` is its own element."""
    return name.split("--", 1)[0] if "--" in name else name


# --- example markup ----------------------------------------------------------


class Tree(HTMLParser):
    """Minimal element tree; text is kept, comments are not."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=False)
        self.root: dict = {"tag": None, "attrs": [], "children": []}
        self.stack: list[dict] = [self.root]

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        # <option>, <li>, <td>… close the open sibling of the same name; the
        # manifests rely on that (`<option value="Astro">` has no end tag).
        if tag in IMPLICIT_CLOSE and self.stack[-1]["tag"] == tag:
            self.stack.pop()
        node = {"tag": tag, "attrs": attrs, "children": []}
        self.stack[-1]["children"].append(node)
        if tag not in VOID_TAGS:
            self.stack.append(node)

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.stack[-1]["children"].append({"tag": tag, "attrs": attrs, "children": []})

    def handle_endtag(self, tag: str) -> None:
        for index in range(len(self.stack) - 1, 0, -1):
            if self.stack[index]["tag"] == tag:
                del self.stack[index:]
                return

    def handle_data(self, data: str) -> None:
        self.stack[-1]["children"].append(data)

    def handle_entityref(self, name: str) -> None:
        self.stack[-1]["children"].append(f"&{name};")

    def handle_charref(self, name: str) -> None:
        self.stack[-1]["children"].append(f"&#{name};")


class Sanitizer:
    """The subset of shell/src/plugins/sanitize.ts an example can hit."""

    def __init__(
        self,
        rules: dict[str, set[str]],
        classes: set[str],
        replaced: set[str],
        supported_hooks: set[str],
    ) -> None:
        self.rules = rules
        self.classes = classes
        self.replaced = replaced
        self.supported_hooks = supported_hooks
        self.dropped: set[str] = set()
        # html.parser lowercases attribute names; inside an SVG subtree the
        # sanitizer matches them VERBATIM, so `viewBox` has to come back.
        self.svg_case = {name.lower(): name for name in rules["SVG_ATTRS"]}

    def attr_ok(self, tag: str, name: str, value: str, in_svg: bool) -> bool:
        rules = self.rules
        if in_svg:
            return name in rules["SVG_ATTRS"]
        if name == "src":
            return value.startswith(("https://", "http://", "sb-asset:"))
        if name == "href":
            return tag == "a" and value.startswith(("https://", "http://"))
        if tag == "video" and name in rules["VIDEO_ATTRS"]:
            return True
        # Per-tag special cases live in sanitize.ts, not in the rule tables
        # this script parses — so they have to be mirrored here.
        if name == "type":
            if tag == "button":
                return value in rules["BUTTON_TYPES"]
            return tag == "input" and value in rules["INPUT_TYPES"]
        # Several <details> sharing a name become an exclusive accordion.
        if tag == "details" and name == "name":
            return True
        if tag == "form" and name in rules["FORM_BLOCKED_ATTRS"]:
            return False
        if tag in rules["CONTROL_TAGS"] and name in rules["CONTROL_ATTRS"]:
            return True
        if name in rules["NATIVE_INTERACTION_ATTRS"]:
            return name != "command" or value in rules["COMMAND_VALUES"]
        # Keep only hooks the shell's behaviour layer actually implements.
        # All other renamed format48 hooks would document inert markup.
        if name.startswith("data-sb-"):
            return name in self.supported_hooks
        return name in rules["ALLOWED_ATTRS"] or re.fullmatch(r"data-[a-z0-9-]+", name)

    def clean_attrs(self, tag: str, attrs: list, in_svg: bool) -> list[tuple[str, str]]:
        out = []
        for raw_name, raw in attrs:
            name = self.svg_case.get(raw_name, raw_name) if in_svg else raw_name
            value = raw if raw is not None else ""
            if name == "src":
                value = EXAMPLE_SRC_MAP.get(value, value)
            if name == "href" and not value.startswith("http"):
                value = EXAMPLE_HREF
            if name == "class":
                value = self.clean_classes(value)
                if not value:
                    continue
            elif not self.attr_ok(tag, name, value, in_svg):
                self.dropped.add(f"{tag}[{name}]")
                continue
            out.append((name, value))
        return out

    def clean_classes(self, value: str) -> str:
        kept = []
        for token in value.split():
            mapped = (
                EXAMPLE_CLASS_MAP.get(token, token) if token in self.replaced else token
            )
            if not mapped:
                self.dropped.add(f".{token}")
                continue
            if mapped not in self.classes:
                self.dropped.add(f".{mapped}")
                continue
            if mapped not in kept:
                kept.append(mapped)
        return " ".join(kept)

    def render(self, node: dict | str, in_svg: bool = False) -> str:
        if isinstance(node, str):
            return node
        tag = node["tag"]
        if tag is None:
            return "".join(self.render(child, in_svg) for child in node["children"])
        in_svg = in_svg or tag == "svg"
        allowed = self.rules["SVG_TAGS"] if in_svg else self.rules["ALLOWED_TAGS"]
        if tag not in allowed:
            self.dropped.add(tag)
            return "".join(self.render(child, in_svg) for child in node["children"])
        attrs = "".join(
            f' {name}="{value}"' if value else f" {name}"
            for name, value in self.clean_attrs(tag, node["attrs"], in_svg)
        )
        if tag in VOID_TAGS:
            return f"<{tag}{attrs}>"
        inner = "".join(self.render(child, in_svg) for child in node["children"])
        return f"<{tag}{attrs}>{inner}</{tag}>"


def parse(html: str) -> dict:
    tree = Tree()
    tree.feed(html)
    tree.close()
    return tree.root


def collapse(html: str) -> str:
    """One line, no indentation runs — the contract is read, not diffed."""
    return re.sub(r"\s*\n\s*", "", html).strip()


def find_with_class(node: dict, name: str) -> dict | None:
    for child in node["children"]:
        if not isinstance(child, dict):
            continue
        classes = {key: value or "" for key, value in child["attrs"]}.get("class", "")
        if name in classes.split():
            return child
        found = find_with_class(child, name)
        if found is not None:
            return found
    return None


def short_example(
    sanitizer: Sanitizer, root: dict, name: str, role: str, root_class: str | None
) -> tuple[str, bool]:
    """`<h3 class="sb-card__title">…</h3>` for one class of a component.

    Returns the markup and whether it had to be invented because the class
    does not appear in the component's own example.
    """
    node = find_with_class(root, name)
    if node is not None:
        tag = node["tag"]
        attrs = sanitizer.clean_attrs(tag, node["attrs"], False)
        rendered = "".join(
            f' {key}="{value}"' if value else f" {key}" for key, value in attrs
        )
        invented = False
    else:
        # The class is not in the component's own example, so place it: a
        # `--modifier` sits on its own base element, a bare state class
        # (is-selected, …) on the component root, an `__element` alone.
        owner = base_class(name)
        if owner == name:
            owner = root_class or "" if role in {"variant", "size", "state"} else ""
        host = find_with_class(root, owner) if owner else None
        tag = host["tag"] if host is not None else "div"
        pair = owner in sanitizer.classes if owner else False
        classes = f"{owner} {name}" if pair else name
        rendered = f' class="{classes}"'
        invented = True
    markup = f"<{tag}{rendered}>" if tag in VOID_TAGS else f"<{tag}{rendered}>…</{tag}>"
    for parent in reversed(PARENTS.get(tag, ())):
        markup = f"<{parent}>{markup}</{parent}>"
    return markup, invented


# --- entries -----------------------------------------------------------------


def rename(text: str) -> str:
    """`f48-` becomes `sb-`, and tokens follow the harvest's mapping.

    A naive rename would produce `--sb-ok` for `--f48-ok`, but the harvest
    maps that onto smabar's `--sb-success`. A purpose naming a token that
    resolves to nothing is caught by check_purpose — this is what keeps it
    from happening in the first place.
    """
    for source, target in TOKEN_ORDER:
        text = text.replace(source, target)
    for source, target in HARVEST.CLASS_MAP.items():
        text = re.sub(rf"(?<![\w-]){re.escape(source)}(?![\w-])", target, text)
    return text.replace("f48-", "sb-")


def purpose_of(component: dict, entry: dict) -> str:
    name = rename(entry["class"])
    if name in PURPOSE_OVERRIDES:
        return PURPOSE_OVERRIDES[name]
    if entry["role"] == "root":
        text = rename(component["description"])
    else:
        note = rename(entry.get("note") or "").strip()
        label = component["name"]
        if not note:
            # No note in the manifest: the class name's own suffix is the
            # only honest description ("sb-card__body" -> "Card: body").
            note = re.split(r"__|--", rename(entry["class"]))[-1].replace("-", " ")
        text = f"{label}: {note}"
    for pattern, replacement in NOTE_FIXES:
        text = re.sub(pattern, replacement, text)
    return re.sub(r"\s+", " ", text).strip().rstrip(";").strip()


def check_purpose(
    name: str, purpose: str, classes_in_css: set[str], tokens: set[str]
) -> None:
    """Nothing a purpose names may be missing from the kit.

    A description that points at a class the harvest dropped, or at a token
    that resolves to nothing, is worse than no description: the agent writes
    markup that renders as plain text and has no way to find out why.
    """
    if NOTE_GUARD.search(purpose):
        sys.exit(f"{name}: purpose still points at JavaScript: {purpose}")
    for token in re.findall(r"--sb-[a-z0-9-]+", purpose):
        if token not in tokens:
            sys.exit(f"{name}: purpose names {token}, which resolves to nothing")
    for referenced in re.findall(r"(?<![-\w])(sb-[a-z0-9-]+)", purpose):
        if referenced not in classes_in_css or referenced in DROP_CLASSES:
            sys.exit(f"{name}: purpose names undocumented .{referenced}")


def build(source: Path) -> tuple[list[dict], dict[str, list[str]]]:
    rules = ts_sets(RULES_TS.read_text())
    harvested = HARVESTED_CSS.read_text()
    classes_in_css = css_classes(harvested, KIT_CSS.read_text())
    layered = layered_props(harvested)
    unlayered = unlayered_props(KIT_CSS.read_text())
    supported_hooks = behaviour_hooks()
    replaced = {
        name
        for name, own in layered.items()
        if own
        and own
        <= (
            unlayered.get(name, set())
            | (
                unlayered.get(base_class(name), set())
                if base_class(name) != name
                else set()
            )
        )
        # An exact un-layered rule is an intentional implementation of this
        # public class, not a base-rule collision that makes it useless.
        and not own & unlayered.get(name, set())
    }
    documented = {
        entry["name"] for entry in json.loads(CONTRACT.read_text())["classes"]
    }
    # Every --sb-* a purpose may name: a theme token, a variable the kit sets
    # itself, or a component knob — read WITH a fallback, so setting it is
    # optional and reading it always resolves.
    tokens = set(json.loads((ROOT / "themes/default.json").read_text()))
    tokens.update(re.findall(r"^\s*(--sb-[a-z0-9-]+)\s*:", harvested, re.MULTILINE))
    tokens.update(re.findall(r"var\(\s*(--sb-[a-z0-9-]+)\s*,", harvested))

    entries: list[dict] = []
    skipped: dict[str, list[str]] = {
        "no behaviour": [],
        "not in css": [],
        "overridden": [],
        "already documented": [],
        "script-built": [],
        "example markup": [],
        "invented example": [],
    }
    seen: set[str] = set()

    for file in COMPONENTS:
        path = source / "src/manifest" / f"{file}.json"
        if not path.is_file():
            sys.exit(f"missing manifest: {path}")
        for component in json.loads(path.read_text()):
            if component["requiresJs"] and component["slug"] not in BEHAVIOUR:
                skipped["no behaviour"].append(component["slug"])
                continue
            category = component["category"]
            category = CATEGORY_MAP.get(category, category)
            if category not in CATEGORIES:
                sys.exit(f"unknown category {category} in {component['slug']}")
            sanitizer = Sanitizer(rules, classes_in_css, replaced, supported_hooks)
            root = parse(rename(component["html"]))
            full = collapse(sanitizer.render(root))
            root_names = [
                rename(member["class"])
                for member in component["classes"]
                if member["role"] == "root"
            ]
            root_class = root_names[0] if root_names else None
            for member in component["classes"]:
                name = rename(member["class"])
                if name in seen or name in documented:
                    if name not in seen:
                        skipped["already documented"].append(name)
                    continue
                if name in DROP_CLASSES:
                    skipped["script-built"].append(name)
                    continue
                if name not in classes_in_css:
                    skipped["not in css"].append(name)
                    continue
                own = layered.get(name, set())
                blocked = unlayered.get(name, set()) | (
                    unlayered.get(base_class(name), set())
                    if base_class(name) != name
                    else set()
                )
                # A class is useless only when the un-layered kit replaces
                # every declaration it owns. Partial overlap is expected:
                # e.g. floating panels get smabar's opaque background while
                # the harvest still supplies their geometry and interaction.
                if own and own <= blocked and not own & unlayered.get(name, set()):
                    skipped["overridden"].append(name)
                    continue
                seen.add(name)
                if name in EXAMPLE_OVERRIDES:
                    example = EXAMPLE_OVERRIDES[name]
                elif member["role"] == "root":
                    # A component with several roots (sb-cols-2/-3/-4) shows
                    # only one of them; point the example at this one.
                    example = re.sub(
                        r'class="(sb-[a-z0-9-]+)"',
                        lambda match, name=name, root_names=root_names, full=full: (
                            f'class="{name}"'
                            if match.group(1) in root_names and name not in full
                            else match.group(0)
                        ),
                        full,
                        count=1,
                    )
                else:
                    example, invented = short_example(
                        sanitizer, root, name, member["role"], root_class
                    )
                    if invented:
                        skipped["invented example"].append(name)
                purpose = purpose_of(component, member)
                check_purpose(name, purpose, classes_in_css, tokens)
                entries.append(
                    {
                        "name": name,
                        "purpose": purpose,
                        "example": example,
                        "category": category,
                    }
                )
            skipped["example markup"].extend(
                f"{component['slug']}: {what}" for what in sorted(sanitizer.dropped)
            )
    entries.sort(key=lambda entry: (entry["category"], entry["name"]))
    return entries, skipped


def var_calls(text: str) -> list[tuple[str, str | None]]:
    """Returns balanced var() names and optional fallback expressions."""
    out: list[tuple[str, str | None]] = []
    for match in re.finditer(r"var\(\s*(--sb-[a-z0-9-]+)", text):
        depth = 1
        index = match.end()
        comma: int | None = None
        while index < len(text) and depth:
            char = text[index]
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            elif char == "," and depth == 1 and comma is None:
                comma = index
            index += 1
        fallback = None
        if comma is not None and depth == 0:
            fallback = re.sub(r"\s+", " ", text[comma + 1 : index - 1]).strip()
        out.append((match.group(1), fallback or None))
    return out


def css_rules(css: str) -> list[tuple[str, str]]:
    """Returns concrete CSS selectors and their bodies through nested at-rules."""
    rules: list[tuple[str, str]] = []
    for prelude, body in HARVEST.split_blocks(css):
        normalized = re.sub(r"\s+", " ", prelude).strip()
        if normalized.startswith("@property"):
            continue
        if normalized.startswith("@"):
            rules.extend(css_rules(body))
        elif normalized:
            rules.append((normalized, body))
    return rules


def top_level_declarations(body: str) -> str:
    """Drops nested blocks so declarations stay assigned to their own rule."""
    out: list[str] = []
    depth = 0
    for char in body:
        if char == "{":
            depth += 1
        elif char == "}":
            depth = max(0, depth - 1)
        elif depth == 0:
            out.append(char)
    return "".join(out)


def source_locations(
    locations: set[tuple[str, str]],
) -> list[dict[str, str]]:
    return [
        {"file": file, "selector": selector} for file, selector in sorted(locations)
    ]


def css_consumer_entries(
    token_consumers: dict[tuple[str, str], set[str]],
) -> list[dict[str, str | list[str]]]:
    return [
        {
            "file": file,
            "selector": selector,
            "fallbacks": sorted(values),
        }
        for (file, selector), values in sorted(token_consumers.items())
    ]


def token_type(
    name: str,
    default: str | None,
    dependencies: list[str],
    known_types: dict[str, str],
) -> str:
    """Classify a token using its CSS value, semantic name, then aliases."""
    value = default or ""
    if name.startswith("--sb-z-"):
        return "integer"
    if "url(" in value or name.endswith(("-icon", "-image")):
        return "image"
    if "gradient" in name or re.search(r"(?:linear|radial|conic)-gradient\(", value):
        return "gradient"
    if "shadow" in name or name.endswith("-glow"):
        return "shadow"
    if "ease" in name:
        return "easing"
    if "font-family" in name:
        return "font-family"
    if re.search(r"(?:color|bg|border|text|track|scrim|empty)$", name) or any(
        marker in value
        for marker in ("color-mix(", "light-dark(", "oklch(", "rgb(", "#")
    ):
        return "color"
    if re.fullmatch(r"-?(?:\d+(?:\.\d+)?|\.\d+)(?:ms|s)", value):
        return "duration"
    if re.fullmatch(r"-?(?:\d+(?:\.\d+)?|\.\d+)(?:deg|grad|rad|turn)", value):
        return "angle"
    if re.fullmatch(r"-?(?:\d+(?:\.\d+)?|\.\d+)%", value) or (
        "%" in value
        and not re.search(
            r"(?:rem|em|px|vh|vw|vmin|vmax|dvh|dvb|ch|ex|cap|lh)\b", value
        )
    ):
        return "percentage"

    inherited = {known_types[item] for item in dependencies if item in known_types}
    if len(inherited) == 1:
        return inherited.pop()

    if value == "none":
        return "css-value"
    if re.search(r"(?:offset|context-[xy])$", name):
        return "dimension"
    if re.search(r"(?:cols|lines)$", name):
        return "integer"
    if re.fullmatch(r"-?\d+(?:\.\d+)?", value):
        return "number"
    if re.search(
        r"(?:gap|height|width|size|min|max|padding|radius|overlap|offset|"
        r"blur|thickness|indent|space|font-size)$",
        name,
    ) or re.search(r"(?:rem|em|px|vh|vw|vmin|vmax|dvh|dvb|ch|ex|cap|lh)\b", value):
        return "dimension"
    return "css-value"


def token_allowed(name: str, kind: str) -> dict[str, str | int | float]:
    if name == "--sb-code-max-height":
        return {"syntax": "none or <length> or dimension expression"}
    syntax = {
        "angle": "<angle>",
        "color": "<color>",
        "dimension": "<length> or dimension expression",
        "duration": "<time>",
        "easing": "<easing-function>",
        "font-family": "<font-family>",
        "gradient": "<gradient>",
        "image": "data: or https: CSS <image>",
        "integer": "<integer>",
        "number": "<number>",
        "percentage": "<percentage>",
        "shadow": "<shadow-list>",
        "css-value": "CSS value described by meaning",
    }
    allowed: dict[str, str | int | float] = {"syntax": syntax[kind]}
    if name.startswith("--sb-opacity-"):
        allowed.update({"minimum": 0, "maximum": 1})
    elif name.startswith("--sb-weight-"):
        allowed.update({"minimum": 1, "maximum": 1000})
    elif name.startswith("--sb-leading-") and kind == "number":
        allowed.update({"minimum": 0.5, "maximum": 3})
    elif kind == "percentage":
        allowed.update({"minimum": 0, "maximum": 100})
    return allowed


def token_meaning(name: str, internal: bool) -> str:
    label = name.removeprefix("--sb-").replace("-", " ")
    if internal:
        return f"Internal runtime or geometry value for {label}."
    return f"Optional component-level override for {label}."


def production_shell_sources() -> tuple[Path, ...]:
    return tuple(
        path
        for path in sorted((ROOT / "shell/src").rglob("*"))
        if path.suffix in {".css", ".ts", ".tsx"} and ".test." not in path.name
    )


def build_component_tokens() -> tuple[dict, dict[str, dict]]:
    """Census every --sb-* property used by shipped CSS and runtime code."""
    sources = production_shell_sources()
    texts = {
        path: re.sub(
            r"^\s*//.*$", "", strip_comments(path.read_text()), flags=re.MULTILINE
        )
        for path in sources
    }
    contract = json.loads(THEME_CONTRACT.read_text())
    base_tokens = contract["baseTokens"]
    base = set(base_tokens)
    names: set[str] = set()
    fallbacks: dict[str, set[str]] = {}
    declarations: dict[str, list[str]] = {}
    declaration_locations: dict[str, dict[str, set[tuple[str, str]]]] = {}
    consumers: dict[str, set[str]] = {}
    css_consumers: dict[str, dict[tuple[str, str], set[str]]] = {}
    for path, source in texts.items():
        relative = str(path.relative_to(ROOT))
        for match in re.finditer(r"--sb-[a-z0-9-]+", source):
            name = match.group(0)
            if name.endswith("-"):
                continue
            names.add(name)
            consumers.setdefault(name, set()).add(relative)
        for name, fallback in var_calls(source):
            if fallback is not None:
                fallbacks.setdefault(name, set()).add(fallback)
        if path.suffix != ".css":
            for match in re.finditer(
                r"@property\s+(--sb-[a-z0-9-]+)\s*\{(.*?)\}", source, re.DOTALL
            ):
                initial = re.search(r"initial-value:\s*([^;'}]+)", match.group(2))
                if initial is not None:
                    declarations.setdefault(match.group(1), []).append(
                        initial.group(1).strip()
                    )
            continue
        for selector, body in css_rules(source):
            declarations_body = top_level_declarations(body)
            for name, fallback in var_calls(declarations_body):
                location = (relative, selector)
                css_consumers.setdefault(name, {}).setdefault(location, set())
                if fallback is not None:
                    css_consumers[name][location].add(fallback)
            for match in re.finditer(
                r"^\s*(--sb-[a-z0-9-]+)\s*:\s*([^;]+);",
                declarations_body,
                re.MULTILINE,
            ):
                name = match.group(1)
                value = re.sub(r"\s+", " ", match.group(2)).strip()
                names.add(name)
                declarations.setdefault(name, []).append(value)
                declaration_locations.setdefault(name, {}).setdefault(value, set()).add(
                    (relative, selector)
                )

    base_consumers = {
        name: {
            "consumers": sorted(consumers.get(name, set())),
            "cssConsumers": css_consumer_entries(css_consumers.get(name, {})),
        }
        for name in sorted(base)
    }

    public: dict[str, dict] = {}
    internal: dict[str, dict] = {}
    definitions: dict[str, tuple[str | None, str | None, list[str], list[str]]] = {}
    for name in sorted(names - base):
        fallback_values = sorted(fallbacks.get(name, set()))
        declared_values = sorted(set(declarations.get(name, [])))
        candidates = fallback_values or declared_values
        representative = (candidates or [None])[0]
        default = representative if len(candidates) <= 1 else None
        dependencies = sorted(
            {
                dependency
                for value in (*fallback_values, *declared_values)
                for dependency in re.findall(r"--sb-[a-z0-9-]+", value)
                if dependency != name
            }
        )
        definitions[name] = (default, representative, dependencies, candidates)

    known_types = {name: definition["type"] for name, definition in base_tokens.items()}
    unresolved = dict(definitions)
    while unresolved:
        progressed = False
        for name, (_, representative, dependencies, _) in list(unresolved.items()):
            missing = [item for item in dependencies if item in unresolved]
            if missing:
                continue
            known_types[name] = token_type(
                name, representative, dependencies, known_types
            )
            del unresolved[name]
            progressed = True
        if not progressed:
            for name, (_, representative, dependencies, _) in unresolved.items():
                known_types[name] = token_type(
                    name, representative, dependencies, known_types
                )
            break

    for name, (default, _, dependencies, candidates) in definitions.items():
        is_internal = INTERNAL_TOKEN.match(name) is not None
        kind = known_types[name]
        family = name.removeprefix("--sb-").split("-", 1)[0]
        uses = css_consumer_entries(css_consumers.get(name, {}))
        item = {
            "scope": "runtime" if is_internal else "component",
            "themeable": not is_internal and name not in declarations,
            "type": kind,
            "default": default,
            "allowed": token_allowed(name, kind),
            "meaning": token_meaning(name, is_internal),
            "group": f"{'runtime' if is_internal else 'component'}.{family}",
            "consumers": sorted(consumers.get(name, set())),
            "cssConsumers": uses,
            "dependencies": dependencies,
        }
        if len(candidates) > 1:
            contextual = []
            for value in candidates:
                locations = {
                    location
                    for location, values in css_consumers.get(name, {}).items()
                    if value in values
                }
                locations.update(declaration_locations.get(name, {}).get(value, set()))
                contextual.append(
                    {"value": value, "consumers": source_locations(locations)}
                )
            item["contextualDefaults"] = contextual
        if is_internal:
            item["nonThemeable"] = True
            internal[name] = item
        else:
            public[name] = item
    payload = {
        "generatedBy": "scripts/gen-kit-classes.py — do not hand-edit",
        "publicComponentTokens": public,
        "internalTokens": internal,
    }
    expected = {
        "--sb-chart-max": "number",
        "--sb-code-max-height": "css-value",
        "--sb-copy-icon": "image",
        "--sb-datepicker-radius": "dimension",
        "--sb-menu-radius": "dimension",
        "--sb-rating-star": "image",
        "--sb-spotlight-blur": "dimension",
        "--sb-stepper-check": "image",
        "--sb-tree-line": "color",
    }
    for name, kind in expected.items():
        definition = public.get(name) or internal.get(name, {})
        if definition.get("type") != kind:
            sys.exit(f"component token classifier drifted: {name} must be {kind}")
    if "--sb-probe" not in internal:
        sys.exit("runtime token census drifted: --sb-probe must be internal")
    return payload, base_consumers


def sync_base_consumers(consumers: dict[str, dict], check: bool) -> int:
    contract = json.loads(THEME_CONTRACT.read_text())
    changed = 0
    for name, metadata in consumers.items():
        definition = contract["baseTokens"][name]
        for field, value in metadata.items():
            if definition.get(field) == value:
                continue
            if check:
                sys.exit(
                    f"{THEME_CONTRACT} has stale {field} for {name} — "
                    "re-run scripts/gen-kit-classes.py"
                )
            definition[field] = value
            changed += 1
    if changed:
        THEME_CONTRACT.write_text(
            json.dumps(contract, indent=2, ensure_ascii=False) + "\n"
        )
    return changed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    checks = parser.add_mutually_exclusive_group()
    checks.add_argument("--check", action="store_true")
    checks.add_argument(
        "--check-local",
        action="store_true",
        help="check the locally reproducible token census without format48 sources",
    )
    args = parser.parse_args()

    token_payload, base_consumers = build_component_tokens()
    token_text = json.dumps(token_payload, indent=2, ensure_ascii=False) + "\n"
    if args.check_local:
        if not TOKENS_OUTPUT.is_file() or TOKENS_OUTPUT.read_text() != token_text:
            sys.exit(f"{TOKENS_OUTPUT} is stale — re-run scripts/gen-kit-classes.py")
        sync_base_consumers(base_consumers, True)
        print("local design-token census up to date")
        return

    entries, skipped = build(args.source)
    payload = {
        "generatedBy": "scripts/gen-kit-classes.py — do not hand-edit",
        "classes": entries,
    }
    text = json.dumps(payload, indent=2, ensure_ascii=False) + "\n"
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_text() != text:
            sys.exit(f"{OUTPUT} is stale — re-run scripts/gen-kit-classes.py")
        if not TOKENS_OUTPUT.is_file() or TOKENS_OUTPUT.read_text() != token_text:
            sys.exit(f"{TOKENS_OUTPUT} is stale — re-run scripts/gen-kit-classes.py")
        sync_base_consumers(base_consumers, True)
        print("kit class docs and design-token census up to date")
        return
    OUTPUT.write_text(text)
    TOKENS_OUTPUT.write_text(token_text)
    changed_consumers = sync_base_consumers(base_consumers, False)
    print(f"wrote {OUTPUT.relative_to(ROOT)} ({len(entries)} classes)")
    print(
        f"wrote {TOKENS_OUTPUT.relative_to(ROOT)} "
        f"({len(token_payload['publicComponentTokens'])} public, "
        f"{len(token_payload['internalTokens'])} internal)"
    )
    print(f"updated {changed_consumers} base-token consumer lists")
    per_category: dict[str, int] = {}
    for entry in entries:
        per_category[entry["category"]] = per_category.get(entry["category"], 0) + 1
    for category, count in sorted(per_category.items()):
        print(f"  {count:4d}  {category}")
    for reason, names in skipped.items():
        if names:
            print(f"  skipped ({reason}): {', '.join(sorted(set(names)))}")


if __name__ == "__main__":
    main()
