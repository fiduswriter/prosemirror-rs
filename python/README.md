# prosemirror-rs — Python bindings

Python bindings for [`prosemirror`](https://crates.io/crates/prosemirror), a Rust
implementation of [ProseMirror](https://prosemirror.net)'s document model and
transform pipeline.

## Installation

```bash
pip install prosemirror-rs
```

## Design goals

- **Zero unnecessary copies.** The schema and document live entirely in Rust
  memory. Only JSON strings cross the Python/Rust boundary.
- **Wire-efficient.** Steps arriving as JSON (e.g. from a WebSocket) can be
  passed directly to `apply_steps_json()` without any Python-level parsing.
- **Database-efficient.** `doc_json()` serializes the document in Rust and
  returns a plain Python `str`, ready to write to a database with no
  intermediate Python objects.

## Quick start

```python
from prosemirror_rs import Editor
import json

schema_json = json.dumps({
    "nodes": {
        "doc":       {"content": "paragraph+"},
        "paragraph": {"content": "text*", "group": "block"},
        "text":      {"group": "inline"},
    },
    "marks": {"strong": {}, "em": {}},
})

doc_json = json.dumps({
    "type": "doc",
    "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Hello"}]}],
})

editor = Editor(schema_json, doc_json)
print(editor.version)   # 0

# Typical server loop: steps arrive as raw JSON from a client
async def on_message(raw: str):
    data = json.loads(raw)                          # parse envelope only
    results = editor.apply_steps_json(              # steps stay as JSON string
        json.dumps(data["steps"])
    )
    if all(results):
        doc = editor.doc_json()                     # serialised in Rust
        await db.execute("UPDATE docs SET body = $1", doc)
```

## API reference

### `Editor(schema_json, doc_json)`

Create an editor. Both arguments are JSON strings (schema spec and initial
document). Raises `ValueError` on malformed input.

### `editor.apply_step(step_json) -> bool`

Apply one step supplied as a JSON string. Returns `True` on success, `False`
if the step cannot be applied (document is left unchanged). Raises `ValueError`
on invalid JSON.

### `editor.apply_steps_json(steps_json, *, stop_on_failure=True) -> list[bool]`

**Preferred method for incoming network data.** Accepts a JSON *array* of
steps as a single string — passed directly to Rust and parsed there, so
nothing touches Python's JSON machinery. Returns one `bool` per step.

### `editor.apply_steps(steps, *, stop_on_failure=True) -> list[bool]`

Convenience method for when steps are constructed or modified in Python. Each
element of `steps` is a JSON string for one step. All steps are parsed before
any are applied, so a bad JSON string raises `ValueError` without mutating
the document. Returns one `bool` per step.

### `editor.doc_json() -> str`

Return the current document as a compact JSON string. Serialized entirely in
Rust; only the final string is handed to Python — suitable for direct database
writes with no intermediate objects.

### `editor.version` *(int, read-only property)*

Number of steps successfully applied since construction. Use as a document
version counter in collaborative-editing protocols.

## Credits

The underlying Rust library was originally written by
**Daniel Seiler** ([Xiphoseer](https://github.com/Xiphoseer), <me@dseiler.eu>),
who designed and implemented the document model, transform pipeline, and
runtime schema system. **Mustafa J** (<mustafa.0x@gmail.com>) has made
substantial contributions to the core Rust implementation.
Currently maintained by
**Johannes Wilm** ([FidusWriter](https://fiduswriter.org),
<johannes@fiduswriter.org>).

ProseMirror is by **Marijn Haverbeke** and contributors —
see [prosemirror.net](https://prosemirror.net).

The Python test suite used for cross-implementation validation is taken from
[`prosemirror-py`](https://github.com/fellowapp/prosemirror-py), a Python port
of ProseMirror originally created by **Shen Li** and maintained by
**Samuel Cormier-Iijima**, **Patrick Gingras**, and others at
[Fellow Insights Inc.](https://github.com/fellowapp)

## License

MIT — see [LICENSE](../LICENSE).

Copyright 2026 Johannes Wilm
Copyright 2025–2026 Mustafa J
Copyright 2020 Daniel Seiler
Copyright 2019 Fellow Insights Inc. (prosemirror-py Python port)
Copyright 2015–2026 Marijn Haverbeke and others
