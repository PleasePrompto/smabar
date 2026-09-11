"""Agent-visible commands. Runtime validation lives in the same domain service as UI."""

TEXT = {"type": "string"}
ID = {"type": "string", "minLength": 1, "maxLength": 128}
REVISION = {"type": "integer", "minimum": 1}
WRITE = {
    "requestId": {
        **ID,
        "description": "Unique operation ID. Reuse after timeout with identical arguments.",
    }
}
EXISTING = {**WRITE, "id": ID, "revision": REVISION}
FIELDS = {
    "title": {"type": "string", "minLength": 1, "maxLength": 240},
    "note": {"type": "string", "maxLength": 10000},
    "remindAt": {
        "type": ["string", "null"],
        "description": "ISO timestamp with timezone offset; null removes reminder.",
    },
    "remindInMinutes": {"type": "integer", "minimum": 1, "maximum": 525600},
}
TODO = {
    "type": "object",
    "properties": {
        "id": ID,
        "title": TEXT,
        "note": TEXT,
        "status": {"enum": ["open", "done"]},
        "revision": REVISION,
        "createdMs": {"type": "integer"},
        "updatedMs": {"type": "integer"},
        "reminderAtMs": {"type": ["integer", "null"]},
        "reminderId": {"type": ["string", "null"]},
        "shownAtMs": {"type": ["integer", "null"]},
        "deliveryState": {
            "enum": ["none", "pending", "shown", "dismissed", "suppressed", "dropped"]
        },
    },
}
OUTPUT = {"type": "object", "properties": {"todo": TODO}, "required": ["todo"]}
COMMANDS = {
    "todos.list": (
        "List todos with pagination.",
        {
            "status": {"enum": ["open", "done", "all"]},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100},
            "offset": {"type": "integer", "minimum": 0},
        },
        [],
        {
            "type": "object",
            "properties": {
                "todos": {"type": "array", "items": TODO},
                "nextOffset": {"type": ["integer", "null"]},
            },
            "required": ["todos", "nextOffset"],
        },
    ),
    "todos.get": ("Read the latest todo and revision.", {"id": ID}, ["id"], OUTPUT),
    "todos.create": (
        "Create and commit a todo. Relative reminders use the host computer's clock.",
        {**WRITE, **FIELDS},
        ["requestId", "title"],
        OUTPUT,
    ),
    "todos.update": (
        "Edit a todo at the revision you read. Supply either remindAt or remindInMinutes.",
        {**EXISTING, **FIELDS},
        ["requestId", "id", "revision"],
        OUTPUT,
    ),
    "todos.complete": (
        "Complete a todo and cancel its reminder.",
        EXISTING,
        list(EXISTING),
        OUTPUT,
    ),
    "todos.reopen": (
        "Reopen a todo without reactivating its old reminder.",
        EXISTING,
        list(EXISTING),
        OUTPUT,
    ),
    "todos.delete": (
        "Delete the addressed todo and cancel its reminder.",
        EXISTING,
        list(EXISTING),
        {"type": "object", "properties": {"deletedId": ID}, "required": ["deletedId"]},
    ),
    "todos.snooze": (
        "Move the reminder to now plus minutes; commit before acknowledging.",
        {**EXISTING, "minutes": {"type": "integer", "minimum": 1, "maximum": 525600}},
        [*EXISTING, "minutes"],
        OUTPUT,
    ),
}
