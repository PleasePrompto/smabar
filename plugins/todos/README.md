# Todos

Lokale Aufgaben und Erinnerungen, bedienbar im Plugin und über MCP.
Die SQLite-Datenbank liegt in `app.data_dir / "todos.sqlite3"`.
Während das Plugin läuft ausschließlich dessen Befehle verwenden, nicht die
Datenbank von außen ändern. Außer dem mitgelieferten SDK sind keine Pakete nötig.

## Für Agenten

Mit `plugin_commands({"id":"todos"})` die aktuellen Schemas lesen. Beispiel
für `plugin_call`:

```json
{
  "id": "todos",
  "command": "todos.create",
  "arguments": {
    "requestId": "release-review-1",
    "title": "Release prüfen",
    "remindInMinutes": 20
  }
}
```

Erfolg bestätigt einen SQLite-Commit und liefert Aufgabe, Revision und absoluten
Erinnerungszeitpunkt. Bei Timeout dieselbe `requestId` mit denselben Argumenten
wiederholen; eine neue ID könnte die Änderung doppelt ausführen. Bestehende
Aufgaben vor Änderungen lesen und deren Revision mitsenden.

Befehle: `todos.list`, `todos.get`, `todos.create`, `todos.update`,
`todos.complete`, `todos.reopen`, `todos.delete`, `todos.snooze`.
`remindAt` benötigt einen ISO-Zeitpunkt mit Zeitzonenoffset; `null` entfernt
die Erinnerung. Relativ geht es mit `remindInMinutes`; Snooze verwendet `minutes`.

## Erinnerungen

Fällige Aufgaben zeigen ein dauerhaftes Popup mit Erledigen und Snooze.
Das X schließt nur die Erinnerung. Snooze beträgt standardmäßig 20 Minuten
und ist einstellbar. Wiederöffnen einer Aufgabe stellt ihren alten Reminder
nicht erneut her. Mehr als fünf gleichzeitig fällige Aufgaben werden zusammengefasst.

Bei gestoppter App oder schlafendem Computer erscheint nichts. Der nächste Start
und die sekündliche Prüfung holen fällige Erinnerungen nach. Ein Absturz zwischen
Anzeige und Speicherung kann eine erneute Anzeige verursachen; unterdrückte
Benachrichtigungen werden nach 30 Sekunden erneut versucht. Ein bereits bestätigter
Ton wird bei Wiederaufbau des Popups nicht erneut abgespielt.

Der lokale Ton liegt unter `sounds/reminder.wav`; das Plugin liest seinen
Codeordner nur. Eigene Dateien gehören in `app.data_dir`. Ton lässt sich im
Plugin und global unter System → Plugin-Audio steuern.

## Prüfen

Aus `sdk/python/`, getrennt vom Repository-Gesamtgate:

```bash
uv run pytest -q ../../plugins/todos/test_model.py ../../plugins/todos/test_views.py
```

Die UI nutzt das gemeinsame Kit einschließlich `data-sb-temporal` für Datum/Zeit.
Das vollständige Markup-/Verhaltensschema über `ui_kit` nachschlagen; dieses
optionale Plugin ist ein Beispiel, keine Voraussetzung für Hostfähigkeiten.
