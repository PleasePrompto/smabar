"""Manual plugin checks: accessible editors and safe interactive markup."""

from html.parser import HTMLParser
import json
from pathlib import Path

import pytest

from views import flyout, reminder, tile


class Markup(HTMLParser):
    def __init__(self, source: str):
        super().__init__()
        self.nodes: list[tuple[str, dict[str, str | None]]] = []
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        self.nodes.append((tag, dict(attrs)))


@pytest.mark.parametrize("language", ["en", "de"])
def test_editors_have_labels_examples_and_owned_native_validation(language):
    translations = json.loads((Path(__file__).parent / "locales" / f"{language}.json").read_text())
    html = flyout(
        {"todos": [], "nextOffset": None},
        translations.__getitem__,
        "open",
        None,
        0,
        False,
    )
    nodes = Markup(html).nodes
    ids = {attrs["id"] for _, attrs in nodes if "id" in attrs}
    for tag, field in nodes:
        if tag not in ("input", "textarea"):
            continue
        assert any(tag == "label" and attrs.get("for") == field["id"] for tag, attrs in nodes)
        if field.get("type") == "text" or tag == "textarea":
            assert field["placeholder"]
        for described in (field.get("aria-describedby") or "").split():
            assert described in ids
    assert any(tag == "form" and attrs.get("class") == "sb-stack" for tag, attrs in nodes)
    assert any(
        tag == "input" and attrs.get("type") == "datetime-local" and "data-sb-temporal" in attrs
        for tag, attrs in nodes
    )
    assert translations["reminderHint"] in html
    assert translations["emptyOpenTitle"] in html
    assert translations["emptyDoneTitle"] in flyout(
        {"todos": [], "nextOffset": None},
        translations.__getitem__,
        "done",
        None,
        0,
        False,
    )


def test_user_content_is_escaped_and_actions_keep_the_exact_revision():
    translations = json.loads((Path(__file__).parent / "locales" / "en.json").read_text())
    todo = {
        "id": "test-id",
        "revision": 7,
        "status": "open",
        "title": "<script>alert(1)</script>",
        "note": "One\nTwo <img src=x>",
        "reminderAtMs": None,
        "deliveryState": "none",
    }
    page = {"todos": [todo], "nextOffset": None}
    for html in [
        flyout(page, translations.__getitem__, "open", todo, 0, True),
        reminder(todo, translations.__getitem__, 20),
        tile(1, None, translations.__getitem__),
    ]:
        nodes = Markup(html).nodes
        assert not any(tag in ("script", "img") for tag, _ in nodes)
        assert not any("style" in attrs for _, attrs in nodes)
        for tag, attrs in nodes:
            if tag == "button" and attrs.get("data-action") in (
                "complete",
                "snooze",
                "delete",
            ):
                assert json.loads(attrs["data-value"]) == {
                    "id": "test-id",
                    "revision": 7,
                }
            if "sb-btn-icon" in (attrs.get("class") or "").split():
                assert attrs["aria-label"] and attrs["title"]
