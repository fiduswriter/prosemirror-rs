# Outstanding work: full prosemirror-model/prosemirror-transform API exposure

This document lists the remaining gaps between what the Node.js and Python
bindings currently expose and the full public API documented at
<https://prosemirror.net/docs/ref/>.

## Known limitations (not just missing — need design work)

### `nodesBetween` / `descendants` early-termination

The bindings use a collect-first approach: all nodes are gathered into a `Vec`
before the Python/JS callback is invoked.  This means returning `false` from
the callback to stop traversal has no effect.  Fixing this requires calling the
Python/JS callback inline from within the Rust traversal, which needs careful
handling of the GIL / napi env lifetimes.
