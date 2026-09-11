# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Crypto price tile: spot prices with 24h change from the free CoinGecko API.

Endpoint: https://api.coingecko.com/api/v3/simple/price — works without an API
key. The keyless public tier is IP-rate-limited to roughly 5-15 requests/min
(docs.coingecko.com → Rate Limits); the Demo tier with a key allows 30 req/min
and 10,000 calls/month. Polling every 5 minutes stays far below both.

Response shape (verified live 2026-08-21):
{"bitcoin": {"eur": 66005, "eur_24h_change": 7.34}, ...} — unknown coin ids
are silently missing from the object (an all-unknown request returns {}),
while an unsupported vs_currency yields present-but-empty entries like
{"bitcoin": {}}.

The flyout sparkline and each tile's statSplit trend are fed from a history of
fetched spot prices (one point per successful poll per coin) — simple/price has
no history endpoint. That history
is cached in app.data_dir, so a restart shows the previous curve immediately
instead of an empty tile that needs four hours to fill up again.
"""

import html
import json
import math
import os
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, datetime
from pathlib import Path

from smabar_sdk import Plugin

NO_DATA = "–"

app = Plugin()

API = "https://api.coingecko.com/api/v3/simple/price"
SEARCH_API = "https://api.coingecko.com/api/v3/search"
MAX_RESULTS = 6
POLL_SECONDS = 300.0  # 5 min — conservative for the keyless CoinGecko tier
DEFAULT_COINS = ["bitcoin", "ethereum"]
DEFAULT_CURRENCY = "eur"
HISTORY_MAX = 48  # ~4h of 5-min polls per coin

# Display symbols for common CoinGecko ids; anything else shows its id.
SYMBOLS = {
    "bitcoin": "BTC",
    "ethereum": "ETH",
    "tether": "USDT",
    "binancecoin": "BNB",
    "solana": "SOL",
    "ripple": "XRP",
    "usd-coin": "USDC",
    "cardano": "ADA",
    "dogecoin": "DOGE",
    "avalanche-2": "AVAX",
    "polkadot": "DOT",
    "tron": "TRX",
    "chainlink": "LINK",
    "litecoin": "LTC",
    "monero": "XMR",
    "stellar": "XLM",
}
CURRENCY_SIGNS = {"eur": "€", "usd": "$", "gbp": "£", "jpy": "¥", "btc": "₿"}


def symbol_of(coin: str) -> str:
    return state["symbols"].get(coin) or SYMBOLS.get(coin, coin)

FETCH_ERRORS = (urllib.error.URLError, TimeoutError, TypeError, ValueError)

# Rows from the last successful fetch plus the current error/form state.
# "applied" mirrors the settings this plugin wrote itself, so the core's
# settings.changed echo of our own set_settings does not trigger a second fetch.
# "history" keeps recent spot prices per coin id for the flyout sparkline.
state: dict = {
    "rows": [],
    "currency": DEFAULT_CURRENCY,
    "error": "",
    "form_error": "",
    "last_ok": "",
    "applied": None,
    "history": {},
    # Per-coin price at the last alert (or first sighting): alerts fire when
    # the price moved more than alertPercent from this anchor, then re-anchor.
    "alert_base": {},
    # The add form: the last search and its CoinGecko matches.
    "query": "",
    "results": [],
    # Display symbols learned from search results, so a new coin shows "SOL".
    "symbols": {},
}

ALERT_DEFAULT_PERCENT = 0.5

# Everything worth surviving a restart. app.data_dir is the writable directory
# this plugin owns: writing here never reloads the plugin, and the file goes
# away with it.
CACHE_FILE = "prices.json"
CACHE_KEYS = ("rows", "currency", "last_ok", "history", "alert_base")


def _cache_number(value: object) -> bool:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return False
    try:
        return math.isfinite(float(value))
    except OverflowError:
        return False


def _valid_cached_row(value: object) -> bool:
    if (
        not isinstance(value, dict)
        or not {"id", "known", "price", "change"} <= value.keys()
    ):
        return False
    return (
        isinstance(value["id"], str)
        and bool(value["id"].strip())
        and isinstance(value["known"], bool)
        and (value["price"] is None or _cache_number(value["price"]))
        and (value["change"] is None or _cache_number(value["change"]))
    )


def _valid_cached_history(value: object) -> bool:
    return isinstance(value, dict) and all(
        isinstance(coin, str)
        and bool(coin.strip())
        and isinstance(points, list)
        and all(_cache_number(point) for point in points)
        for coin, points in value.items()
    )


def _valid_cached_alerts(value: object) -> bool:
    return isinstance(value, dict) and all(
        isinstance(coin, str) and bool(coin.strip()) and _cache_number(price)
        for coin, price in value.items()
    )


def _valid_cache(value: object) -> bool:
    if not isinstance(value, dict) or not all(key in value for key in CACHE_KEYS):
        return False
    rows = value["rows"]
    return (
        isinstance(rows, list)
        and all(_valid_cached_row(row) for row in rows)
        and isinstance(value["currency"], str)
        and bool(value["currency"].strip())
        and isinstance(value["last_ok"], str)
        and _valid_cached_history(value["history"])
        and _valid_cached_alerts(value["alert_base"])
    )


def _warn_invalid_cache(path: Path, error: object) -> None:
    app.log(
        "warn",
        "ignoring an invalid price cache; starting without cached prices",
        error=f"{type(error).__name__}: {error}",
        hint=(
            f"remove {path.name} or fix its permissions; "
            "the next successful price update will replace it"
        ),
    )


def load_cache() -> bool:
    """Restores the last known prices. True when something was restored."""
    path = app.data_dir / CACHE_FILE
    try:
        cached = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return False
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        _warn_invalid_cache(path, error)
        return False
    if not _valid_cache(cached):
        _warn_invalid_cache(
            path, "expected complete rows/currency/last_ok/history/alert_base data"
        )
        return False
    state.update({key: cached[key] for key in CACHE_KEYS})
    symbols = cached.get("symbols")
    if isinstance(symbols, dict) and all(
        isinstance(coin, str) and isinstance(symbol, str) for coin, symbol in symbols.items()
    ):
        state["symbols"] = symbols
    return bool(state["rows"])


def save_cache() -> None:
    """Write + rename: a plugin killed mid-write must not leave half a file."""
    path = app.data_dir / CACHE_FILE
    temporary = path.with_suffix(".tmp")
    try:
        temporary.write_text(
            json.dumps({**{key: state[key] for key in CACHE_KEYS}, "symbols": state["symbols"]}),
            encoding="utf-8",
        )
        os.replace(temporary, path)
    except OSError as error:
        app.log("warn", "could not cache the prices", error=str(error))


def alert_percent() -> float:
    """Alert threshold in percent; 0 or negative disables price popups."""
    raw = app.settings.get("alertPercent", ALERT_DEFAULT_PERCENT)
    try:
        return float(raw)
    except (TypeError, ValueError):
        app.log(
            "warn", 'settings key "alertPercent" is not a number; using the default'
        )
        return ALERT_DEFAULT_PERCENT


WARN_ICON = (
    "<span data-lucide='triangle-alert' aria-hidden='true' class='sb-warn'></span>"
)


def esc(text: str) -> str:
    return html.escape(text)


def coin_ids() -> list[str]:
    raw = app.settings.get("coins", DEFAULT_COINS)
    if not isinstance(raw, list):
        app.log("warn", 'settings key "coins" is not a list; using the default coins')
        return list(DEFAULT_COINS)
    return [str(coin).strip().lower() for coin in raw if str(coin).strip()]


def currency() -> str:
    return str(app.settings.get("currency") or "").strip().lower() or DEFAULT_CURRENCY


def request_prices(ids: list[str], cur: str) -> dict:
    """One simple/price call; raises one of FETCH_ERRORS on failure."""
    query = urllib.parse.urlencode(
        {"ids": ",".join(ids), "vs_currencies": cur, "include_24hr_change": "true"}
    )
    with urllib.request.urlopen(f"{API}?{query}", timeout=10) as response:
        payload = json.load(response)
    if not isinstance(payload, dict):
        raise TypeError(f"unexpected response shape: {type(payload).__name__}")
    return payload


def search_coins(query: str) -> list[dict]:
    """CoinGecko /search: coins matching a name or symbol, best match first."""
    params = urllib.parse.urlencode({"query": query})
    with urllib.request.urlopen(f"{SEARCH_API}?{params}", timeout=8) as response:
        payload = json.load(response)
    coins = payload.get("coins") if isinstance(payload, dict) else None
    if not isinstance(coins, list):
        raise TypeError("unexpected search response shape")
    found = []
    for coin in coins[:MAX_RESULTS]:
        if not isinstance(coin, dict) or not isinstance(coin.get("id"), str):
            continue
        rank = coin.get("market_cap_rank")
        found.append(
            {
                "id": coin["id"],
                "name": str(coin.get("name") or coin["id"]),
                "symbol": str(coin.get("symbol") or "").upper(),
                "rank": rank if isinstance(rank, int) and not isinstance(rank, bool) else None,
            }
        )
    return found


def priced_row(coin: str, entry: object, cur: str) -> dict:
    """Row for one coin. price None + known False: id unknown to CoinGecko;
    price None + known True: the vs_currency is not supported."""
    price = entry.get(cur) if isinstance(entry, dict) else None
    change = entry.get(f"{cur}_24h_change") if isinstance(entry, dict) else None
    return {
        "id": coin,
        "known": isinstance(entry, dict),
        "price": float(price) if _cache_number(price) else None,
        "change": float(change) if _cache_number(change) else None,
    }


def record_history(ids: list[str]) -> None:
    """Appends the fetched spot prices and drops coins no longer tracked."""
    history = state["history"]
    for row in state["rows"]:
        if row["price"] is not None:
            points = history.setdefault(row["id"], [])
            points.append(row["price"])
            del points[:-HISTORY_MAX]
    state["history"] = {coin: points for coin, points in history.items() if coin in ids}


def fetch_all() -> None:
    """Refreshes all configured coins; on failure keeps the last known rows."""
    ids = coin_ids()
    cur = currency()
    if not ids:
        state["rows"] = []
        state["currency"] = cur
        state["error"] = ""
        render_all()
        return
    try:
        payload = request_prices(ids, cur)
    except FETCH_ERRORS as error:
        state["error"] = str(error) or type(error).__name__
        app.log(
            "warn",
            "price fetch failed; showing the last known prices",
            error=str(error),
            coins=",".join(ids),
            hint="check network connectivity; HTTP 429 means the free CoinGecko rate limit was hit",
        )
        render_all()
        return
    state["rows"] = [priced_row(coin, payload.get(coin), cur) for coin in ids]
    state["currency"] = cur
    state["error"] = ""
    state["last_ok"] = datetime.now(UTC).astimezone().strftime("%H:%M")
    record_history(ids)
    save_cache()
    notify_price_moves()
    if any(row["known"] and row["price"] is None for row in state["rows"]):
        app.log(
            "warn",
            "CoinGecko does not support the configured currency",
            currency=cur,
            hint="pick a code from https://api.coingecko.com/api/v3/simple/supported_vs_currencies",
        )
    for row in state["rows"]:
        if not row["known"]:
            app.log(
                "warn",
                "CoinGecko returned no data for a configured coin id",
                coin=row["id"],
                hint="unknown id — remove it in the flyout or fix the settings",
            )
    app.log("info", "prices updated", coins=",".join(ids), currency=cur)
    render_all()


def save_coins(coins: list[str]) -> None:
    """Persists the coin list, remembers the snapshot, reloads prices."""
    app.set_settings({**app.settings, "coins": coins})
    state["applied"] = json.dumps(app.settings, sort_keys=True)
    fetch_all()


def fmt_money(price: float, cur: str) -> str:
    if price >= 1000:
        amount = f"{price:,.0f}"
    elif price >= 1:
        amount = f"{price:,.2f}"
    else:
        amount = f"{price:.4f}"
    sign = CURRENCY_SIGNS.get(cur)
    return f"{sign}{amount}" if sign else f"{amount} {cur.upper()}"


def coin_icon(coin: str) -> str:
    return "bitcoin" if coin == "bitcoin" else "chart-line"


def change_badge(change: float | None) -> str:
    if change is None:
        return f"<span class='sb-badge'>{NO_DATA}</span>"
    up = change >= 0
    variant = "sb-badge-success" if up else "sb-badge-danger"
    icon = "trending-up" if up else "trending-down"
    return (
        f"<span class='sb-badge {variant}'><span data-lucide='{icon}' aria-hidden='true'></span>"
        f"{abs(change):.1f}%</span>"
    )


def notify_price_moves() -> None:
    """Pushes a popup when a price moved past the alertPercent threshold.

    The anchor is the price at the last alert (or first sighting), so slow
    drifts still alert once they add up. Popups have no shell-side rate
    limit — the threshold IS this plugin's frequency control.
    """
    threshold = alert_percent()
    base = state["alert_base"]
    cur = state["currency"]
    for row in state["rows"]:
        price = row["price"]
        if price is None:
            continue
        anchor = base.get(row["id"])
        if anchor is None or anchor <= 0:
            base[row["id"]] = price
            continue
        move = (price - anchor) / anchor * 100.0
        if threshold <= 0 or abs(move) < threshold:
            continue
        base[row["id"]] = price
        up = move >= 0
        icon = "trending-up" if up else "trending-down"
        tone = "sb-ok" if up else "sb-crit"
        symbol = esc(symbol_of(row["id"]))
        app.render(
            "crypto",
            "popup",
            "<div class='sb-inline'>"
            f"<span data-lucide='{icon}' aria-hidden='true' class='{tone}'></span>"
            f"<div><b>{symbol} {'+' if up else ''}{move:.1f}%</b><br>"
            f"<span class='sb-mono sb-dim'>{esc(fmt_money(anchor, cur))}"
            f" → {esc(fmt_money(price, cur))}</span></div></div>",
            ttl_ms=8000,
        )
        app.log("info", "price alert popup sent", coin=row["id"], move=f"{move:+.2f}%")
    tracked = {row["id"] for row in state["rows"]}
    state["alert_base"] = {
        coin: price for coin, price in base.items() if coin in tracked
    }


def change_text(change: float | None) -> str:
    """Sub-line delta as colored text — the badge is too tall for line two."""
    if change is None:
        return f"<span class='sb-muted'>{NO_DATA}</span>"
    up = change >= 0
    cls = "sb-ok" if up else "sb-crit"
    sign = "+" if up else "−"
    return f"<span class='{cls} sb-mono'>{sign}{abs(change):.1f}%</span>"


def spark_points(coin_id: str) -> str:
    """Up to the last 8 recorded spot prices — the tile sparkline's data."""
    points = state["history"].get(coin_id, [])
    return ",".join(f"{point:g}" for point in points[-8:])


def tile_view(row: dict) -> str:
    """One rotator view as a statSplit cover: price/symbol | 24h delta/trend."""
    symbol = esc(symbol_of(row["id"]))
    has_price = row["price"] is not None
    price = esc(fmt_money(row["price"], state["currency"])) if has_price else NO_DATA
    points = spark_points(row["id"])
    # The sparkline needs two recorded polls; until then a quiet "24h" keeps
    # the stack's two-line structure (and the tile width) stable.
    trend = (
        f"<span class='sb-spark sb-accent' data-chart='sparkline'"
        f" data-points='{points}'></span>"
        if "," in points
        else "<span class='sb-muted'>24h</span>"
    )
    return (
        "<div class='sb-inline'>"
        f"<span data-lucide='{coin_icon(row['id'])}' aria-hidden='true'></span>"
        "<span class='sb-tile-split'>"
        "<span class='sb-tile-stack'>"
        f"<span class='sb-mono'>{price}</span>"
        f"<span class='sb-muted' data-marquee style='max-width:5.5rem'>{symbol}</span>"
        "</span>"
        "<span class='sb-tile-stack'>"
        f"<span>{change_text(row['change'])}</span>"
        f"{trend}"
        "</span></span></div>"
    )


def render_tile() -> None:
    rows = state["rows"]
    warn = WARN_ICON if state["error"] else ""
    if not coin_ids():
        inner = (
            "<span data-lucide='wallet' aria-hidden='true'></span>"
            f"<span class='sb-muted'>{esc(app.t('crypto.no_coins'))}</span>"
        )
    elif not rows:
        inner = (
            "<span data-lucide='wallet' aria-hidden='true'></span>"
            f"<span class='sb-muted'>{esc(app.t('crypto.no_data'))}</span>{warn}"
        )
    else:
        views = "".join(tile_view(row) for row in rows)
        # All coins roll upward through the tile; the rotator sizes itself to
        # the widest view, so the tile never resizes mid-rotation.
        if len(rows) > 1:
            views = f"<div data-rotator='up' data-rotator-interval='4000'>{views}</div>"
        inner = f"{views}{warn}"
    # No width of our own: the rotator already sizes to its widest view and
    # sb-mono keeps the digits from re-flowing, so the tile hugs its content.
    app.render("crypto", "tile", f"<div class='sb-tile'>{inner}</div>")


def render_header() -> str:
    return (
        "<div class='sb-header'>"
        "<span class='sb-icon-badge'><span data-lucide='bitcoin' aria-hidden='true'></span></span>"
        f"<span class='sb-title'>{esc(app.t('crypto.title'))}</span>"
        "<div class='sb-header-actions'>"
        f"<button class='sb-btn sb-btn-icon' data-action='refresh'"
        f" title='{esc(app.t('crypto.refresh'))}' aria-label='{esc(app.t('crypto.refresh'))}'>"
        "<span data-lucide='refresh-cw' aria-hidden='true'></span></button></div></div>"
    )


def render_hero() -> str:
    """Accent hero for the top coin plus its price sparkline once history exists."""
    rows = state["rows"]
    if not rows or rows[0]["price"] is None:
        return ""
    first = rows[0]
    symbol = esc(symbol_of(first["id"]))
    name = symbol if symbol.lower() == first["id"] else f"{symbol} · {esc(first['id'])}"
    hero = (
        "<div class='sb-hero'>"
        f"<small>{name}</small>"
        f"<h1>{esc(fmt_money(first['price'], state['currency']))}</h1>"
        f"{change_badge(first['change'])}</div>"
    )
    points = state["history"].get(first["id"], [])
    if len(points) >= 2:
        data = ",".join(f"{value:.6g}" for value in points)
        hero += (
            f"<div class='sb-spark' data-chart='sparkline' data-points='{data}'></div>"
        )
    return hero


def render_row(row: dict) -> str:
    coin = esc(row["id"])
    symbol = esc(symbol_of(row["id"]))
    name = f"<b>{symbol}</b>"
    if symbol.lower() != row["id"]:
        name += f" <span class='sb-faint'>{coin}</span>"
    if row["price"] is None:
        problem = "crypto.bad_currency" if row["known"] else "crypto.unknown_id"
        value = f"<span class='sb-badge sb-badge-warning'>{esc(app.t(problem))}</span>"
    else:
        value = (
            f"<span class='sb-mono'>{esc(fmt_money(row['price'], state['currency']))}</span>"
            f" {change_badge(row['change'])}"
        )
    return (
        f"<div class='sb-row'>{name}"
        f"<span class='sb-push'>{value}</span>"
        f"<button class='sb-btn sb-btn-icon sb-btn-ghost' data-action='remove'"
        f" data-value='{coin}' title='{esc(app.t('crypto.remove'))}'"
        f" aria-label='{esc(app.t('crypto.remove'))}'>"
        "<span data-lucide='x' aria-hidden='true'></span></button></div>"
    )


def result_row(coin: dict) -> str:
    rank = f" · #{coin['rank']}" if coin["rank"] else ""
    if coin["id"] in coin_ids():
        control = f"<span class='sb-badge sb-badge-success'>{esc(app.t('crypto.tracked'))}</span>"
    else:
        control = (
            f"<button type='button' class='sb-btn sb-btn-ghost sb-btn-icon' data-action='add'"
            f" data-value='{esc(coin['id'])}' aria-label='{esc(app.t('crypto.add'))}'"
            f" title='{esc(app.t('crypto.add'))}'><span data-lucide='plus' aria-hidden='true'></span></button>"
        )
    return (
        "<div class='sb-row'><span data-lucide='chart-line' aria-hidden='true'></span>"
        f"<span>{esc(coin['name'])}<br><small class='sb-faint'>{esc(coin['symbol'])}{esc(rank)}</small></span>"
        f"<span class='sb-push'></span>{control}</div>"
    )


def render_form() -> str:
    """Search by name or symbol; a result row adds the coin by its CoinGecko id."""
    form = (
        "<form class='sb-stack'><div class='sb-field-stack'>"
        f"<label class='sb-field__label' for='crypto-query'>{esc(app.t('crypto.add_coin'))}</label>"
        "<div class='sb-field'>"
        f"<input class='sb-input' id='crypto-query' data-field='query' type='search'"
        f" value='{esc(state['query'])}' placeholder='{esc(app.t('crypto.search_placeholder'))}'"
        " aria-describedby='crypto-query-hint'>"
        "<button type='submit' class='sb-btn sb-btn-primary' data-action='search'>"
        f"<span data-lucide='search' aria-hidden='true'></span>{esc(app.t('crypto.search'))}</button></div>"
        f"<p class='sb-field__hint' id='crypto-query-hint'>{esc(app.t('crypto.search_hint'))}</p>"
        "</div></form>"
    )
    rows = "".join(result_row(coin) for coin in state["results"])
    form += f"<div class='sb-reveal{' sb-active' if rows else ''}'><div class='sb-list'>{rows}</div></div>"
    if state["form_error"]:
        form += (
            "<div class='sb-alert sb-alert--warn' role='alert'>"
            f"<span class='sb-alert__icon'>{WARN_ICON}</span>"
            f"<div><p class='sb-alert__text'>{esc(state['form_error'][:160])}</p></div></div>"
        )
    elif state["query"] and not rows:
        form += f"<p class='sb-error'>{esc(app.t('crypto.no_results'))}</p>"
    return form


def render_footer() -> str:
    if state["error"]:
        return (
            "<div class='sb-tip'>"
            f"{WARN_ICON}<span>{esc(app.t('crypto.fetch_failed'))}:"
            f" {esc(state['error'][:120])}<br>"
            f"<span class='sb-faint'>{esc(app.t('crypto.last_success'))}:"
            f" {esc(state['last_ok'] or NO_DATA)}</span></span></div>"
        )
    return (
        f"<p class='sb-meta'>"
        f"{esc(app.t('crypto.updated'))} {esc(state['last_ok'] or NO_DATA)}</p>"
    )


def render_flyout() -> None:
    if state["rows"]:
        body = (
            render_hero()
            + f"<div class='sb-section'>{esc(app.t('crypto.coins'))}</div>"
            + f"<div class='sb-list'>{''.join(render_row(row) for row in state['rows'])}</div>"
        )
    elif not coin_ids():
        hint = esc(app.t("crypto.no_coins_hint"))
        body = f"<div class='sb-card'><p class='sb-dim'>{hint}</p></div>"
    else:
        body = (
            "<div class='sb-card'>"
            f"<p class='sb-dim'>{esc(app.t('crypto.no_data'))}</p>"
            f"<small>{esc(app.t('crypto.offline_hint'))}</small></div>"
        )
    app.render(
        "crypto", "flyout", render_header() + body + render_form() + render_footer()
    )


def hover_row(row: dict) -> str:
    """Read-only overview line for the hover preview — no buttons here."""
    symbol = esc(symbol_of(row["id"]))
    if row["price"] is None:
        value = f"<span class='sb-badge sb-badge-warning'>{NO_DATA}</span>"
    else:
        value = (
            f"<span class='sb-mono'>{esc(fmt_money(row['price'], state['currency']))}</span>"
            f" {change_badge(row['change'])}"
        )
    return (
        f"<div class='sb-row'><b>{symbol}</b><span class='sb-push'>{value}</span></div>"
    )


def render_hover() -> None:
    """Custom hover preview (target "hover"): a compact glance at every coin.

    Hovering shows THIS instead of the click flyout; the click flyout keeps
    the hero, the coin management and the add form.
    """
    rows = state["rows"]
    if rows:
        body = (
            f"<div class='sb-section'>{esc(app.t('crypto.hover_title'))}</div>"
            f"<div class='sb-list'>{''.join(hover_row(row) for row in rows)}</div>"
        )
    else:
        body = f"<div class='sb-card'><p class='sb-dim'>{esc(app.t('crypto.no_data'))}</p></div>"
    body += f"<p class='sb-meta'>{esc(app.t('crypto.hover_hint'))}</p>"
    app.render("crypto", "hover", body)


def render_all() -> None:
    render_tile()
    render_hover()
    render_flyout()


@app.on_ready
def show_cached_prices() -> None:
    """Render the cached prices BEFORE the first request goes out, so the bar
    is never blank while the network is slow — or down."""
    if load_cache():
        app.log("info", "restored cached prices", coins=len(state["rows"]))
        render_all()


@app.every(POLL_SECONDS)
def poll() -> None:
    fetch_all()


@app.on_action("crypto", "refresh")
def refresh(action: str, value: object) -> None:
    fetch_all()


@app.on_action("crypto", "search")
def search(action: str, value: object) -> None:
    query = str(value.get("query", "") if isinstance(value, dict) else "").strip()
    state.update(query=query, results=[], form_error="")
    if query:
        try:
            state["results"] = search_coins(query)
        except FETCH_ERRORS as error:
            state["form_error"] = f"{app.t('crypto.search_failed')}: {error}"
            app.log(
                "warn",
                "coin search failed",
                query=query,
                error=str(error),
                hint="check network connectivity; HTTP 429 means the free CoinGecko rate limit was hit",
            )
    render_flyout()


@app.on_action("crypto", "add")
def add(action: str, value: object) -> None:
    """Adds a coin picked from the search results — its id is CoinGecko's own."""
    coin = str(value or "").strip()
    match = next((c for c in state["results"] if c["id"] == coin), None)
    if match is None:
        return
    state.update(query="", results=[], form_error="")
    if match["symbol"]:
        state["symbols"][coin] = match["symbol"]
    ids = coin_ids()
    if coin in ids:
        render_flyout()
        return
    app.log("info", "coin added", coin=coin)
    save_coins([*ids, coin])


@app.on_action("crypto", "remove")
def remove(action: str, value: object) -> None:
    coin = str(value or "")
    ids = coin_ids()
    if coin not in ids:
        return
    app.log("info", "coin removed", coin=coin)
    save_coins([c for c in ids if c != coin])


@app.on_settings_changed
def settings_changed(settings: dict) -> None:
    if json.dumps(settings, sort_keys=True) == state["applied"]:
        return  # echo of our own set_settings; fetch_all already ran
    state["applied"] = json.dumps(settings, sort_keys=True)
    fetch_all()


if __name__ == "__main__":
    app.run()
