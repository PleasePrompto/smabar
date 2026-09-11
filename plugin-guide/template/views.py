"""Markup for the folder-watch template: semantic HTML on the smabar UI kit.

Every function returns a string the plugin pushes with app.render. Nothing
here touches the SDK, so this module reads and tests without a running bar.
Layout, spacing and type come from the kit's sb-* classes; the only inline
style is a data value.
"""

import json
from collections.abc import Callable
from html import escape

Translator = Callable[[str], str]


def tr(t: Translator, key: str, **values: object) -> str:
    """t() with placeholders: tr(t, "folder.files", count=3)."""
    text = t(key)
    for name, value in values.items():
        text = text.replace("{" + name + "}", str(value))
    return text


def icon(name: str) -> str:
    return f'<span data-lucide="{name}" aria-hidden="true"></span>'


def action(
    name: str,
    label: str,
    value: object = "",
    *,
    primary: bool = False,
    symbol: str = "",
    icon_only: bool = False,
) -> str:
    """A data-action button. Icon-only buttons carry aria-label AND title:
    the shell turns title into its themed tooltip, so the control has a name."""
    encoded = (
        value if isinstance(value, str) else json.dumps(value, separators=(",", ":"))
    )
    classes = "sb-btn sb-btn-primary" if primary else "sb-btn sb-btn-ghost"
    if icon_only:
        classes += " sb-btn-icon"
    accessible = (
        f' aria-label="{escape(label, quote=True)}" title="{escape(label, quote=True)}"'
        if icon_only
        else ""
    )
    content = (icon(symbol) if symbol else "") + ("" if icon_only else escape(label))
    return (
        f'<button type="button" class="{classes}" data-action="{escape(name)}"'
        f' data-value="{escape(encoded, quote=True)}"{accessible}>{content}</button>'
    )


def gb(size: int) -> str:
    return f"{size / 1e9:.1f} GB"


def percent(used: int, limit: int) -> int:
    return min(100, round(100 * used / limit)) if limit > 0 else 0


def tile(layout: str, state: dict, limit: int, folder: str, t: Translator) -> str:
    """The bar tile in one of three covers (ui_kit coverLayouts), by the coverLayout setting."""
    used, files = state["used"], state["files"]
    title = escape(
        tr(t, "folder.tileTitle", folder=folder, used=gb(used), count=files), quote=True
    )
    if layout == "statSplit":
        return (
            f'<div class="sb-tile" title="{title}"><span class="sb-tile-split">'
            f'<span class="sb-tile-stack"><span class="sb-mono" data-sb-tween>{gb(used)}</span>'
            f'<span class="sb-muted">{escape(t("folder.usedLabel"))}</span></span>'
            f'<span class="sb-tile-stack"><span class="sb-mono" data-sb-tween>{files}</span>'
            f'<span class="sb-muted">{escape(t("folder.filesLabel"))}</span></span></span></div>'
        )
    if layout == "basic":
        return f'<div class="sb-tile" title="{title}">{icon("folder")}<span class="sb-mono">{gb(used)}</span></div>'
    # progressRing: the ring fills the tile height, the center shows the bare number.
    return (
        f'<div class="sb-tile" title="{title}"><span class="sb-tile-stack">'
        f'<span class="sb-mono">{gb(used)}</span><span class="sb-muted">{escape(t("folder.title"))}</span></span>'
        f'<span class="sb-gauge" data-chart="donut" data-value="{percent(used, limit)}" data-max="100">'
        f'<span class="sb-mono" data-sb-tween>{percent(used, limit)}</span></span></div>'
    )


def hover(state: dict, limit: int, t: Translator) -> str:
    """The hover preview: one line, no controls."""
    return (
        f'<div class="sb-row">{icon("folder")}'
        f"<span>{escape(tr(t, 'folder.used', used=gb(state['used']), limit=gb(limit)))}</span>"
        f'<span class="sb-faint sb-push">{escape(tr(t, "folder.files", count=state["files"]))}</span></div>'
    )


def flyout(state: dict, folder: str, limit: int, t: Translator) -> str:
    """Header with actions, KPIs, states, and the settings form."""
    header = (
        f'<div class="sb-header"><span class="sb-icon-badge">{icon("folder")}</span>'
        f'<span class="sb-title">{escape(t("folder.title"))}</span>'
        f'<div class="sb-header-actions">{action("refresh", t("folder.refresh"), symbol="refresh-cw", icon_only=True)}</div></div>'
    )
    if state["error"]:
        status = (
            f'<div class="sb-alert sb-alert--danger">{icon("triangle-alert")}'
            f'<span class="sb-alert__text">{escape(state["error"])}</span></div>'
        )
    elif not state["measured_at"]:
        status = (
            f'<div class="sb-empty"><span class="sb-empty__icon">{icon("hourglass")}</span>'
            f'<span class="sb-empty__title">{escape(t("folder.emptyTitle"))}</span>'
            f'<span class="sb-empty__text">{escape(t("folder.emptyHint"))}</span></div>'
        )
    else:
        status = (
            f'<div class="sb-kpi-grid"><div class="sb-kpi"><span class="sb-kpi-value" data-sb-tween>{gb(state["used"])}</span>'
            f'<span class="sb-kpi-label">{escape(tr(t, "folder.used", used="", limit=gb(limit)).strip())}</span></div>'
            f'<div class="sb-kpi"><span class="sb-kpi-value" data-sb-tween>{state["files"]}</span>'
            f'<span class="sb-kpi-label">{escape(t("folder.filesLabel"))}</span></div></div>'
            f'<div class="sb-progress"><span style="width: {percent(state["used"], limit)}%"></span></div>'
            f'<p class="sb-meta">{escape(tr(t, "folder.measuring") if state["busy"] else tr(t, "folder.measuredAt", time=state["measured_at"]))}</p>'
        )
    form = (
        f'<div class="sb-section">{escape(t("folder.settings"))}</div>'
        '<form class="sb-stack">'
        f'<div class="sb-field-stack"><label class="sb-field__label" for="folder">{escape(t("folder.folderLabel"))}</label>'
        f'<input id="folder" class="sb-input" data-field="folder" value="{escape(folder, quote=True)}"'
        f' placeholder="{escape(t("folder.folderPlaceholder"), quote=True)}" aria-describedby="folder-hint">'
        f'<p class="sb-field__hint" id="folder-hint">{escape(t("folder.folderHint"))}</p></div>'
        f'<div class="sb-field-stack"><label class="sb-field__label" for="limit">{escape(t("folder.limitLabel"))}</label>'
        f'<input id="limit" class="sb-input" type="number" min="0.1" step="0.1" data-field="limitGb" value="{limit / 1e9:g}"'
        f' placeholder="{escape(t("folder.limitPlaceholder"), quote=True)}" aria-describedby="limit-hint">'
        f'<p class="sb-field__hint" id="limit-hint">{escape(t("folder.limitHint"))}</p></div>'
        f'<div class="sb-cluster"><button type="submit" class="sb-btn sb-btn-primary" data-action="save">{escape(t("folder.save"))}</button></div>'
        "</form>"
    )
    return header + status + form


def popup(used: int, limit: int, folder: str, t: Translator) -> str:
    """The over-limit toast; its button works like any data-action."""
    return (
        f'<div class="sb-row">{icon("triangle-alert")}'
        f"<span>{escape(tr(t, 'folder.overLimit', folder=folder, used=gb(used), limit=gb(limit)))}</span>"
        f"{action('refresh', t('folder.refresh'), symbol='refresh-cw', icon_only=True)}</div>"
    )
