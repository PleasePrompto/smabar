#!/usr/bin/env python3
"""One-time harvest of the format48 UI component CSS into smabar's plugin kit.

format48 (a separate, unpublished project of the same author) is a flat,
dependency-free, fully tokenized component library. smabar's plugin kit had
the design system but almost no vocabulary, so the components are taken over
rather than rewritten. This script did the mechanical part; the result is
CHECKED IN and maintained here from now on. Re-run it only to pull a specific
component across again.

Four transforms, each reported so the diff is auditable:

1. Tokens      --f48-* -> smabar's --sb-* tokens (TOKEN_MAP). Anything left
                over would fail the anti-drift test that every var(--sb-*) in
                the kit is a real theme token, so unmapped names abort.
2. Scale       format48 is page scale (body 1rem), smabar is bar scale
                (--sb-fs-m is 0.75rem) and a flyout is ~340px wide. Raw rem
                values are mapped onto smabar's own tokens where one exists
                and scaled down where none does.
3. Names       .f48-x -> .sb-x, @keyframes, data-f48-* likewise.
4. Removal     Selectors and declarations that cannot work inside a plugin
                shadow root inside a glass surface: :root/html/body,
                position: fixed, and the Chrome-only properties WebKitGTK
                does not implement.

Usage:  scripts/harvest-f48.py [--source DIR] [--check]
        --check re-runs the transform and fails if the checked-in output
        differs, so a stale harvest cannot go unnoticed.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = Path.home() / "Dokumente/Coding/Webprojekt format48 UI"
OUTPUT = ROOT / "shell/src/styles/kit-components.css"

# Components worth having in a 340px flyout and a 56px tile. Left out on
# purpose: everything requiring JavaScript, the fx-* motion tier (a
# marketing showcase), and the page-scale furniture (navbar, sidebar,
# footer, megamenu, bottombar) that has no meaning inside a tile.
COMPONENTS = [
    # Foundation — the first harvest.
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
    # Second harvest: a plugin author must not run into "smabar cannot do
    # that". Everything below is bar-scale and driven by markup alone or by
    # a behaviour module in shell/src/plugins/behaviour/.
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

# --- 1. Tokens ---------------------------------------------------------------

TOKEN_MAP = {
    # Brand and text
    "--f48-accent": "--sb-accent",
    "--f48-accent-text": "--sb-accent",
    "--f48-on-accent": "--sb-on-accent",
    "--f48-fg": "--sb-text",
    "--f48-fg-muted": "--sb-text-muted",
    # Surfaces. A "surface" is the inner tint on the bar's translucent
    # surfaces, not an opaque panel.
    "--f48-bg": "--sb-flyout-bg",
    "--f48-surface": "--sb-inner-bg",
    "--f48-surface-2": "--sb-surface-2",
    "--f48-border": "--sb-inner-border",
    "--f48-edge": "--sb-edge",
    # Status
    "--f48-ok": "--sb-success",
    "--f48-warn": "--sb-warning",
    "--f48-danger": "--sb-danger",
    "--f48-info": "--sb-info",
    # Shape
    "--f48-radius-sm": "--sb-radius-xs",
    "--f48-radius-md": "--sb-radius-m",
    "--f48-radius-lg": "--sb-radius-m",
    "--f48-radius-xl": "--sb-radius-l",
    "--f48-radius": "--sb-radius-s",
    "--f48-btn-radius": "--sb-btn-radius",
    "--f48-input-radius": "--sb-input-radius",
    # Elevation
    "--f48-shadow-sm": "--sb-shadow-sm",
    "--f48-shadow-lg": "--sb-shadow-lg",
    "--f48-shadow": "--sb-shadow",
    # Spacing, compressed onto smabar's tighter scale
    "--f48-space-1": "--sb-space-s",
    "--f48-space-2": "--sb-space-m",
    "--f48-space-3": "--sb-space-l",
    "--f48-space-4": "--sb-space-xl",
    "--f48-space-5": "--sb-space-xl",
    "--f48-space-6": "--sb-space-xl",
    "--f48-space-8": "--sb-space-xl",
    "--f48-space-10": "--sb-space-xl",
    # Type
    "--f48-font-sans": "--sb-font-sans",
    "--f48-font-display": "--sb-font-sans",
    "--f48-font-mono": "--sb-font-mono",
    # Motion
    "--f48-speed-reveal": "--sb-dur-normal",
    "--f48-speed": "--sb-dur-fast",
    "--f48-ease-spring": "--sb-ease-spring",
    "--f48-ease": "--sb-ease-out",
    # Blur: in-page filter blur only (spoilers, skeletons, image blur-up).
    # smabar draws no backdrop blur, so a component's blur is a light touch.
    "--f48-blur-xs": "--sb-blur-xs",
    "--f48-blur-sm": "--sb-blur-xs",
    "--f48-blur-lg": "--sb-blur",
    "--f48-blur-xl": "--sb-blur",
    "--f48-blur": "--sb-blur",
    "--f48-backdrop-blur": "--sb-blur",
    "--f48-transient-blur": "--sb-blur",
    # Misc
    "--f48-ring": "--sb-focus-ring",
    "--f48-edge-fade": "--sb-edge-fade",
    "--f48-chevron": "--sb-chevron",
}

# Same spelling, different component. smabar's `.sb-field` is the compact
# input + action row used throughout the bar; format48's `.f48-field` is the
# vertical label/control/hint/error anatomy. Keep both instead of letting an
# un-layered row inherit `flex-direction: column` from the harvested class.
# The boundary keeps `.f48-field__hint` and friends unchanged.
CLASS_MAP = {"f48-field": "sb-field-stack"}

# Base class names the hand-written kit owns. Some have different meanings;
# stack/grid/card have compatible families but incompatible base contracts.
# Dropping the exact generated base leaves one semantic owner while keeping
# useful `__element`/`--variant` rules built on smabar's base.
DROP_CLASSES = {
    # format48: a translucent card with a backdrop blur.
    # smabar:  draws no blur at all (2026-09-03); the plain card is enough.
    "sb-card--glass",
    # format48: a page section, padding-block 46 to 69px.
    # smabar:  a small caption label above a group.
    "sb-section",
    # format48: a toggle BUTTON carrying a pressed state.
    # smabar:  a switch.
    "sb-toggle",
    # format48: one member of its alternate `.sb-tabs__list` contract.
    # smabar: one member of the shell-driven `.sb-tabs` contract.
    "sb-tab",
    # The public kit owns these base contracts; generated variants/elements
    # may still build on them.
    "sb-card",
    "sb-btn-group",
    "sb-field-stack",
    "sb-grid",
    "sb-input-group",
    "sb-stack",
}

# Dropping a base class would leave its variants without foundation, so the
# whole family goes. `.sb-toggle-*` is format48's segmented control built on
# the switch name smabar already owns. `.sb-tabs-*` and `.sb-carousel-*` are
# alternate DOM contracts; smabar's shell-driven versions use `.sb-tab` and
# direct carousel children, so mixing either family produces half of each.
# format48: a scroll-driven entrance animation (animation-timeline: view()).
# smabar:  .sb-reveal is .sb-active-driven and hand-written in ui-kit.css.
# A flyout is 340px and does not scroll, so the view() timeline never
# advances — the whole family goes, keyframes included.
DROP_FAMILIES = {"sb-carousel", "sb-section", "sb-tabs", "sb-toggle", "sb-reveal"}

# Attribute-based components that collide with a smabar convention. The
# f48- -> sb- rename turns format48's data-f48-tooltip into data-sb-tooltip,
# which is EXACTLY the attribute smabar's own tooltip layer uses: every
# tooltipped element would render a second, CSS-only tooltip as a
# pseudo-element — clipped by any overflow container and in the wrong colours.
# smabar's version portals out to <body>, which is the whole point, so
# format48's is dropped.
# format48 animates a number up from zero on first viewport entry. A plugin
# flyout re-renders every second, so the animation would restart forever;
# there is no behaviour module for it and the hook would be a dead end.
DROP_ATTRS = {"data-sb-tooltip", "data-sb-countup"}

DROP_EXACT_SELECTORS = {
    ".is-open > .sb-spoiler__content",
    '[aria-expanded="true"] > .sb-spoiler__chevron',
    '[aria-expanded="true"] > .sb-spoiler__more',
    '[aria-expanded="false"] > .sb-spoiler__less',
}

DROP_SELECTOR = re.compile(
    r"\.(?:"
    + "|".join(sorted(DROP_CLASSES))
    + r")(?![\w-])|\.(?:"
    + "|".join(sorted(DROP_FAMILIES))
    + r")[\w-]*|\[(?:"
    + "|".join(sorted(DROP_ATTRS))
    + r")[\w-]*"
)

# A handful of expressions have no meaning in smabar and are rewritten
# rather than dropped, so the rule they belong to keeps working.
REWRITES = [
    # format48 derives a second brand hue from --f48-hue, its rebrand knob.
    # smabar already HAS a second brand colour, so use it and lose the knob.
    (
        "--sb-mesh-hue-2: oklch(from var(--sb-accent) 60% c calc(var(--sb-hue) + 80));",
        "--sb-mesh-hue-2: var(--sb-accent-2);",
    ),
    # A data-URI fill cannot follow the component's foreground. Treat the
    # shared chevron as an alpha mask and paint it with currentColor.
    (
        "background: var(--sb-chevron) center / contain no-repeat;",
        (
            "background-color: currentColor;\n  "
            "-webkit-mask: var(--sb-chevron) center / contain no-repeat;\n  "
            "mask: var(--sb-chevron) center / contain no-repeat;"
        ),
    ),
]

Z_INDEX = {
    "-1": "--sb-z-behind",
    "1": "--sb-z-raised",
    "2": "--sb-z-control",
    "3": "--sb-z-control-active",
    "60": "--sb-z-plugin-menu",
    "80": "--sb-z-plugin-context",
    "100": "--sb-z-plugin-toast",
}

# Component-internal knobs keep their name (renamed to --sb-) and always
# carry a fallback, so they never need to be theme tokens. The kit's
# anti-drift test only inspects var() uses WITHOUT a fallback.
KNOB_PREFIXES = ("--f48-",)

# --- 2. Scale ----------------------------------------------------------------

FONT_SIZES = {
    "0.6875rem": "var(--sb-fs-xs)",
    "0.75rem": "var(--sb-fs-s)",
    "0.8125rem": "var(--sb-fs-s)",
    "0.875rem": "var(--sb-fs-m)",
    "0.95rem": "var(--sb-fs-l)",
    "1rem": "var(--sb-fs-l)",
    "1.1rem": "var(--sb-fs-xl)",
    "1.125rem": "var(--sb-fs-xl)",
    "1.15rem": "var(--sb-fs-xl)",
    "1.2rem": "var(--sb-fs-xl)",
    "1.25rem": "var(--sb-fs-hero)",
}

SPACES = {
    "0.125rem": "var(--sb-space-2xs)",
    "0.25rem": "var(--sb-space-2xs)",
    "0.375rem": "var(--sb-space-xs)",
    "0.5rem": "var(--sb-space-s)",
    "0.75rem": "var(--sb-space-m)",
    "1rem": "var(--sb-space-m)",
    "1.25rem": "var(--sb-space-l)",
    "1.5rem": "var(--sb-space-l)",
    "2rem": "var(--sb-space-xl)",
    "2.5rem": "var(--sb-space-xl)",
    "3rem": "var(--sb-space-xl)",
}

RADII = {
    "0.25rem": "var(--sb-radius-xs)",
    "0.5rem": "var(--sb-radius-s)",
    "0.75rem": "var(--sb-radius-m)",
    "1rem": "var(--sb-radius-m)",
    "1.5rem": "var(--sb-radius-l)",
}

SPACE_PROPS = re.compile(r"^(padding|margin|gap|row-gap|column-gap|inset)")
SIZE_PROPS = re.compile(
    r"^(min-|max-)?(block-size|inline-size|width|height|line-height)$"
)
# Bar scale is roughly three quarters of page scale; anything without a
# matching token is scaled rather than dropped.
SCALE = 0.72

# --- 4. Removal --------------------------------------------------------------

DEAD_SELECTOR = re.compile(r"(^|[\s,>+~])(:root|html|body)\b")
# Chrome-only, verified missing on WebKitGTK 2.52 by the startup capability
# probe (shell/src/ipc/capabilities.ts). Carrying them would be dead weight.
DEAD_PROPS = ("interpolate-size", "field-sizing", "scrollbar-color")
DEAD_VALUES = ("::picker(", "::details-content", "anchor-name", "position-anchor")
DEAD_SELECTORS = ("::picker(", "::details-content")
# `@supports (interpolate-size: allow-keywords)` guards a whole block that can
# never match here. Dropping only the declarations would leave the empty
# guard behind, so the at-rule goes with them.
# Selectors whose elements live in the top layer, where `position: fixed`
# is correct and the UA stylesheet depends on it.
TOP_LAYER = re.compile(r"\bdialog\b|\[popover\]|\.sb-(?:modal|drawer|lightbox)\b")

DEAD_AT_RULE = re.compile(r"^@supports\b.*(?:" + "|".join(DEAD_PROPS) + r")")

FONT_WEIGHTS = {
    "400": "var(--sb-weight-normal, 400)",
    "500": "var(--sb-weight-medium, 500)",
    "550": "var(--sb-weight-medium-strong, 550)",
    "600": "var(--sb-weight-semibold, 600)",
    "650": "var(--sb-weight-strong, 650)",
    "700": "var(--sb-weight-bold, 700)",
    "750": "var(--sb-weight-heavy, 750)",
    "800": "var(--sb-weight-extrabold, 800)",
}
LINE_HEIGHTS = {
    "0.95": "var(--sb-leading-condensed, 0.95)",
    "1": "var(--sb-leading-none, 1)",
    "1.05": "var(--sb-leading-dense, 1.05)",
    "1.1": "var(--sb-leading-compact, 1.1)",
    "1.12": "var(--sb-leading-display, 1.12)",
    "1.15": "var(--sb-leading-ui, 1.15)",
    "1.2": "var(--sb-leading-short, 1.2)",
    "1.25": "var(--sb-leading-heading, 1.25)",
    "1.3": "var(--sb-leading-control, 1.3)",
    "1.35": "var(--sb-leading-help, 1.35)",
    "1.4": "var(--sb-leading-snug, 1.4)",
    "1.45": "var(--sb-leading-body, 1.45)",
    "1.5": "var(--sb-leading-normal, 1.5)",
    "1.55": "var(--sb-leading-reading, 1.55)",
    "1.6": "var(--sb-leading-relaxed, 1.6)",
    "1.7": "var(--sb-leading-loose, 1.7)",
    "1rem": "var(--sb-leading-icon, 0.75rem)",
    "1.25rem": "var(--sb-leading-status, 0.875rem)",
}
LETTER_SPACING = {
    "-0.04em": "var(--sb-tracking-tightest, -0.04em)",
    "-0.03em": "var(--sb-tracking-extra-tight, -0.03em)",
    "-0.02em": "var(--sb-tracking-tighter, -0.02em)",
    "-0.01em": "var(--sb-tracking-tight, -0.01em)",
    "0.01em": "var(--sb-tracking-fine, 0.01em)",
    "0.02em": "var(--sb-tracking-subtle, 0.02em)",
    "0.04em": "var(--sb-tracking-wide, 0.04em)",
    "0.06em": "var(--sb-tracking-wider, 0.06em)",
    "0.08em": "var(--sb-tracking-label)",
    "0.1em": "var(--sb-tracking-widest, 0.1em)",
}
OPACITIES = {
    value: f"var(--sb-opacity-{value.removeprefix('0.')}, {value})"
    for value in ("0.35", "0.4", "0.45", "0.55", "0.6", "0.7", "0.75", "0.85")
}
DESIGN_PROP = re.compile(
    r"^(?:gap|row-gap|column-gap|margin(?:-.+)?|padding(?:-.+)?|"
    r"border-radius|box-shadow|font-size|font-weight|line-height|"
    r"letter-spacing|opacity)$"
)


def component_design_value(prop: str, value: str, selector: str) -> str:
    """Makes remaining component literals explicit, documented token defaults."""
    if not DESIGN_PROP.match(prop) or not selector:
        return value
    if value.startswith(("var(", "calc(var(")):
        return value
    if re.fullmatch(r"(?:0|auto|none|normal|inherit)(?:\s+(?:0|auto|none))*", value):
        return value
    match = re.search(r"\.sb-([a-z0-9_-]+)", selector)
    if match is None:
        return value
    owner = match.group(1).replace("__", "-").replace("--", "-")
    state = re.search(
        r":(hover|active|focus|focus-visible|disabled|checked|before|after)\b",
        selector,
    )
    if state is not None:
        owner = f"{owner}-{state.group(1)}"
    token = f"--sb-{owner}-{prop}"
    return f"var({token}, {value})"


def scale_length(value: str) -> str:
    """Scales a bare rem length onto bar scale, rounded to 1/16rem."""

    def one(match: re.Match[str]) -> str:
        rem = float(match.group(1)) * SCALE
        return f"{round(rem * 16) / 16:g}rem"

    return re.sub(r"(\d*\.?\d+)rem", one, value)


def rewrite_declaration(prop: str, value: str, selector: str = "") -> str | None:
    """Applies the scale map to one declaration; None drops it."""
    if prop in DEAD_PROPS or any(dead in value for dead in DEAD_VALUES):
        return None
    if selector.strip() == ".sb-code__body":
        if prop == "font-size":
            return "var(--sb-fs-s)"
        if prop == "line-height":
            return "var(--sb-leading-control, 1.35)"
    if prop in {"--f48-chart-1", "--f48-chart-2", "--f48-chart-3", "--f48-chart-4"}:
        # The series palette is inherited from the active theme. A local
        # declaration on every chart would make accepted theme overrides inert.
        return None
    if prop == "background-image" and "--f48-chevron" in value:
        # The shared chevron is a colorless mask token. Enhanced selects and
        # comboboxes get a currentColor pseudo-element in ui-kit.css; native
        # opt-outs keep the UA picker instead of painting the mask as an image.
        return None
    if prop in {"background", "background-color"} and "var(--f48-surface-2)" in value:
        # `--sb-surface-2` is an opaque base colour, just like the surface
        # tokens. Component backgrounds must apply the shared alpha at the point
        # of use; otherwise disabled controls, tracks and skeletons stay opaque
        # while the surrounding bar is transparent.
        value = value.replace(
            "var(--f48-surface-2)",
            "color-mix(in srgb, var(--sb-surface-2) var(--sb-bar-opacity), transparent)",
        )
    if prop in {"background", "background-color"} and value.strip() in {
        "var(--f48-surface)",
        "var(--sb-inner-bg)",
    }:
        # Theme surface bases are opaque. Only the bar/flyout/tile/inner
        # families participate in the global opacity setting; ui-kit.css replaces top-layer menus,
        # modals and drawers with their deliberately opaque semantic surfaces.
        return "color-mix(in srgb, var(--sb-inner-bg) var(--sb-bar-opacity), transparent)"
    if prop == "background" and value.strip() == (
        "light-dark(var(--f48-surface), "
        "color-mix(in srgb, var(--sb-surface-2) var(--sb-bar-opacity), transparent))"
    ):
        return (
            "color-mix(in srgb, "
            "light-dark(var(--sb-inner-bg), var(--sb-surface-2)) "
            "var(--sb-bar-opacity), transparent)"
        )
    if prop == "position" and value.strip() == "fixed":
        # An element in the TOP LAYER (an open <dialog>, a [popover]) is
        # lifted out of the layout tree: its containing block is the viewport
        # no matter how many backdrop-filter ancestors it has, and the UA
        # stylesheet already positions it. Overriding that would break it.
        if TOP_LAYER.search(selector):
            return "fixed"
        # Everywhere else `fixed` anchors to a transformed card, not the viewport —
        # the trap CLAUDE.md records. `absolute` is the honest replacement:
        # it re-anchors to the component's own wrapper and, unlike
        # `relative`, still takes no space in the flow.
        return "absolute"
    if prop == "z-index" and value.strip() in Z_INDEX:
        return f"var({Z_INDEX[value.strip()]})"
    if prop == "font-weight" and value.strip() in FONT_WEIGHTS:
        return FONT_WEIGHTS[value.strip()]
    if prop == "line-height" and value.strip() in LINE_HEIGHTS:
        return LINE_HEIGHTS[value.strip()]
    if prop == "letter-spacing" and value.strip() in LETTER_SPACING:
        return LETTER_SPACING[value.strip()]
    if prop == "opacity" and value.strip() in OPACITIES:
        return OPACITIES[value.strip()]
    if prop.startswith(("border", "outline")):
        value = re.sub(
            r"(?<![\w.-])1px(?=\s+(?:solid|dashed|dotted)\b)",
            "var(--sb-border-width, 1px)",
            value,
        )
    if prop == "font-size":
        value = FONT_SIZES.get(value.strip(), scale_length(value))
    elif prop == "border-radius":
        value = " ".join(RADII.get(part, part) for part in value.split())
    elif SPACE_PROPS.match(prop):
        value = " ".join(SPACES.get(part, scale_length(part)) for part in value.split())
    elif SIZE_PROPS.match(prop) and "rem" in value:
        value = scale_length(value)
    return component_design_value(prop, value, selector)


def rename(text: str) -> str:
    """f48- -> sb- for classes, custom properties, keyframes and data-attrs.

    Longest name first: --f48-surface-2 must not be rewritten by the rule for
    --f48-surface, which would silently invent --sb-inner-bg-2.
    """
    for source in sorted(TOKEN_MAP, key=len, reverse=True):
        text = text.replace(source, TOKEN_MAP[source])
    for source, target in CLASS_MAP.items():
        text = re.sub(rf"(?<![\w-]){re.escape(source)}(?![\w-])", target, text)
    text = text.replace("--f48-", "--sb-")
    text = text.replace(".f48-", ".sb-")
    text = text.replace("data-f48-", "data-sb-")
    text = re.sub(r"\bf48-", "sb-", text)
    for source, target in REWRITES:
        text = text.replace(source, target)
    text = re.sub(
        r"background: (?:currentcolor|var\(--sb-text-muted\));\n"
        r"(?P<indent>\s*)mask: var\(--sb-chevron\) center / contain no-repeat;",
        "background-color: currentColor;\n"
        r"\g<indent>-webkit-mask: var(--sb-chevron) center / contain no-repeat;\n"
        r"\g<indent>mask: var(--sb-chevron) center / contain no-repeat;",
        text,
    )
    return text


def split_blocks(css: str) -> list[tuple[str, str]]:
    """Splits flat CSS into (prelude, body) pairs by brace matching."""
    blocks, depth, start, prelude = [], 0, 0, ""
    for index, char in enumerate(css):
        if char == "{":
            if depth == 0:
                prelude = css[start:index].strip()
                start = index + 1
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                blocks.append((prelude, css[start:index]))
                start = index + 1
    return blocks


def strip_comments(css: str) -> str:
    return re.sub(r"/\*.*?\*/", "", css, flags=re.DOTALL)


def transform_body(
    body: str, stats: dict[str, int], indent: str = "  ", selector: str = ""
) -> str:
    """Rewrites one rule's declarations, preserving nested at-rules.

    `@starting-style` and `@media` appear INSIDE rule bodies in this library
    (37 and 242 times), so a body is not just a list of declarations — a
    naive split on ";" produced unbalanced braces.
    """
    out: list[str] = []
    rest = body
    while rest:
        brace = rest.find("{")
        if brace == -1:
            out.extend(declarations(rest, stats, indent, selector))
            break
        # Everything before the nested block is plain declarations; the text
        # after the last ";" is that block's prelude.
        head, prelude = rest[:brace], ""
        if ";" in head:
            cut = head.rindex(";") + 1
            head, prelude = head[:cut], head[cut:]
        else:
            head, prelude = "", head
        out.extend(declarations(head, stats, indent, selector))
        depth, end = 0, brace
        for index in range(brace, len(rest)):
            if rest[index] == "{":
                depth += 1
            elif rest[index] == "}":
                depth -= 1
                if depth == 0:
                    end = index
                    break
        inner = transform_body(rest[brace + 1 : end], stats, indent + "  ")
        if inner:
            out.append(f"{indent}{prelude.strip()} {{\n{inner}\n{indent}}}")
        rest = rest[end + 1 :]
    return "\n".join(out)


def declarations(
    chunk: str, stats: dict[str, int], indent: str, selector: str = ""
) -> list[str]:
    """The declaration lines of a brace-free chunk."""
    out = []
    for raw in chunk.split(";"):
        declaration = raw.strip()
        if not declaration or ":" not in declaration:
            continue
        prop, _, value = declaration.partition(":")
        prop, value = prop.strip(), value.strip()
        rewritten = rewrite_declaration(prop, value, selector)
        if rewritten is None:
            stats["dropped declarations"] += 1
            continue
        if rewritten != value:
            stats["rescaled declarations"] += 1
        out.append(f"{indent}{prop}: {rewritten};")
    return out


def keep_selectors(prelude: str, stats: dict[str, int]) -> str | None:
    """Drops the parts of a selector list that name a replaced class.

    A list may pair a dropped class with kept ones, so only the matching
    parts go and the rule survives for the rest. None means nothing is left.
    """
    parts = prelude.split(",")
    kept = []
    for part in parts:
        renamed = rename(part).strip()
        if (
            renamed
            and renamed not in DROP_EXACT_SELECTORS
            and not DROP_SELECTOR.search(renamed)
            and not any(dead in part for dead in DEAD_SELECTORS)
        ):
            kept.append(part)
    if len(kept) == len(parts):
        return prelude
    stats["dropped collisions"] += 1
    return ",".join(kept).strip() if kept else None


def harvest_at_rule(prelude: str, body: str, stats: dict[str, int]) -> str | None:
    """Transforms one at-rule body. At-rules nest (`@media { @supports { … } }`),
    so an inner at-rule recurses instead of being treated as a selector."""
    nested = []
    for sel, inner_body in split_blocks(body):
        if DEAD_SELECTOR.search(sel) or DEAD_AT_RULE.match(sel):
            stats["dropped rules"] += 1
            continue
        if sel.startswith("@"):
            deeper = harvest_at_rule(sel, inner_body, stats)
            if deeper:
                nested.append(indent(deeper))
            continue
        kept = keep_selectors(sel, stats)
        if kept is None:
            continue
        nested.append(
            f"  {kept} {{\n{transform_body(inner_body, stats, selector=rename(kept))}\n  }}"
        )
    if not nested:
        return None
    return f"{prelude} {{\n" + "\n".join(nested) + "\n}"


def indent(block: str) -> str:
    return "\n".join("  " + line if line else line for line in block.split("\n"))


def harvest_one(path: Path, stats: dict[str, int]) -> str:
    css = strip_comments(path.read_text())
    top = split_blocks(css)
    # Every component file is one `@layer f48.components { … }` wrapper.
    inner = "".join(body for prelude, body in top if prelude.startswith("@layer"))
    pieces = []
    for prelude, body in split_blocks(inner):
        if DEAD_SELECTOR.search(prelude) or DEAD_AT_RULE.match(prelude):
            stats["dropped rules"] += 1
            continue
        prelude = keep_selectors(prelude, stats)
        if prelude is None:
            continue
        if prelude.startswith("@"):
            nested = harvest_at_rule(prelude, body, stats)
            if nested:
                pieces.append(nested)
            continue
        declarations = transform_body(body, stats, selector=rename(prelude))
        if declarations:
            pieces.append(f"{prelude} {{\n{declarations}\n}}")
            stats["rules"] += 1
    return "\n\n".join(pieces)


def top_level_properties(path: Path, stats: dict[str, int]) -> list[str]:
    """Keeps source `@property` registrations outside the component layer."""
    properties = []
    for prelude, body in split_blocks(strip_comments(path.read_text())):
        if not prelude.startswith("@property"):
            continue
        declarations = transform_body(body, stats)
        if declarations:
            properties.append(rename(f"{prelude} {{\n{declarations}\n}}"))
    return properties


def build(source: Path) -> tuple[str, dict[str, int]]:
    stats = dict.fromkeys(
        [
            "rules",
            "dropped rules",
            "dropped collisions",
            "dropped declarations",
            "rescaled declarations",
        ],
        0,
    )
    header = [
        "/* GENERATED by scripts/harvest-f48.py — do not hand-edit this file.",
        " *",
        " * Component CSS harvested from the format48 UI library and mapped onto",
        " * smabar's tokens and bar scale. It sits in a cascade layer so every",
        " * hand-written rule in ui-kit.css (which is un-layered) wins on conflict.",
        " */",
        "@layer sb.reset, sb.components;",
    ]
    files = []
    for name in COMPONENTS:
        path = source / "src/css/components" / f"{name}.css"
        if not path.is_file():
            sys.exit(f"missing component: {path}")
        files.append((name, path))
    # Registrations stay top-level exactly as authored; their order follows
    # the source package order above.
    properties = [
        block for _, path in files for block in top_level_properties(path, stats)
    ]
    parts = [*header, *properties, "@layer sb.components {"]
    for name, path in files:
        parts.append(f"\n/* ---- {name} ---- */\n{rename(harvest_one(path, stats))}")
    parts.append("}")
    css = "\n".join(parts) + "\n"
    leftover = sorted(set(re.findall(r"--f48-[a-z0-9-]+", css)))
    if leftover:
        sys.exit(f"unmapped format48 tokens: {', '.join(leftover)}")
    check_tokens(css)
    return css, stats


def check_tokens(css: str) -> None:
    """Every var(--sb-x) without a fallback must resolve.

    Either it is a theme token (themes/default.json, and the Rust theme test
    guarantees the other four carry it too) or the harvested CSS declares it
    itself — components set their own internal variables on a parent rule and
    read them on children. Anything else would render as nothing, which is
    exactly what the kit's anti-drift test refuses.
    """
    theme = set(json.loads((ROOT / "themes/default.json").read_text()))
    declared = set(re.findall(r"^\s*(--sb-[a-z0-9-]+)\s*:", css, re.MULTILINE))
    declared.update(
        re.findall(
            r"^\s*(--sb-[a-z0-9-]+)\s*:",
            (ROOT / "shell/src/styles/globals.css").read_text(),
            re.MULTILINE,
        )
    )
    used = set(re.findall(r"var\(\s*(--sb-[a-z0-9-]+)\s*\)", css))
    unresolved = sorted(used - theme - declared)
    if unresolved:
        sys.exit(
            "harvested CSS reads tokens that resolve to nothing: "
            + ", ".join(unresolved)
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    css, stats = build(args.source)
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_text() != css:
            sys.exit(f"{OUTPUT} is stale — re-run scripts/harvest-f48.py")
        print("harvest up to date")
        return
    OUTPUT.write_text(css)
    print(f"wrote {OUTPUT.relative_to(ROOT)} ({len(css.splitlines())} lines)")
    for key, value in stats.items():
        print(f"  {value:5d}  {key}")


if __name__ == "__main__":
    main()
