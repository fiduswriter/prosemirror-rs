# prosemirror-rs — Node.js bindings

Node.js bindings for [`prosemirror`](https://crates.io/crates/prosemirror), a Rust
implementation of [ProseMirror](https://prosemirror.net)'s document model and
transform pipeline.

## Installation

```bash
npm install prosemirror-rs
```

## Design goals

- **Zero unnecessary copies.** The schema and document live entirely in Rust
  memory. Only JSON strings cross the JavaScript/Rust boundary.
- **Wire-efficient.** Steps arriving as JSON (e.g. from a WebSocket) can be
  passed directly to `applyStepsJson()` without any JS-level parsing.
- **Database-efficient.** `docJson()` serializes the document in Rust and
  returns a plain JS `string`, ready to write to a database with no
  intermediate objects.

## Quick start

```js
const { Editor } = require('prosemirror-rs');

const schemaJson = JSON.stringify({
    nodes: {
        doc:       { content: 'paragraph+' },
        paragraph: { content: 'text*', group: 'block' },
        text:      { group: 'inline' },
    },
    marks: { strong: {}, em: {} },
});

const docJson = JSON.stringify({
    type: 'doc',
    content: [{ type: 'paragraph', content: [{ type: 'text', text: 'Hello' }] }],
});

const editor = new Editor(schemaJson, docJson);
console.log(editor.version);   // 0

// Typical server loop: steps arrive as raw JSON from a WebSocket client
wss.on('message', (raw) => {
    const data = JSON.parse(raw);                     // parse envelope only
    const ok = editor.applyStepsJson(                 // steps stay as JSON string
        JSON.stringify(data.steps)
    );
    if (ok) {
        db.execute('UPDATE docs SET body = $1', [editor.docJson()]);
    }
});
```

## Building from source

```bash
cd node
npm run build   # runs cargo build --release + copies the .node artifact
npm test        # build + run the test suite
```

## API reference

### `new Editor(schemaJson, docJson)`

Create an editor. Both arguments are JSON strings (schema spec and initial
document). Throws `Error` on malformed input.

### `editor.applyStep(stepJson) → boolean`

Apply one step supplied as a JSON string. Returns `true` on success, `false`
if the step cannot be applied (document is left unchanged). Throws `Error`
on invalid JSON.

### `editor.applyStepsJson(stepsJson) → boolean`

**Preferred method for incoming network data.** Accepts a JSON *array* of
steps as a single string — passed directly to Rust and parsed there, so
nothing touches JS's JSON machinery.

The batch is **atomic**: if any step fails the document and version are rolled
back entirely and `false` is returned. Throws `Error` on invalid JSON.

### `editor.applySteps(steps) → boolean`

Convenience method for when steps are constructed or modified in JavaScript.
Each element of `steps` is a JSON string for one step.

The batch is **atomic**: if any step fails the document and version are rolled
back entirely and `false` is returned. Throws `Error` on invalid JSON.

### `editor.reset(docJson)`

Replace the document with a new one, reusing the already-parsed schema.
Resets the version counter to zero. Throws `Error` on malformed input.

### `editor.docJson() → string`

Return the current document as a compact JSON string. Serialized entirely in
Rust; only the final string is passed to JavaScript — suitable for direct
database writes with no intermediate objects.

### `editor.version` *(number, read-only)*

Number of steps successfully applied since construction or the last `reset()`.
Use as a document version counter in collaborative-editing protocols.

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
