# Upstream JavaScript Test Files

These files are unmodified copies of the compiled JavaScript test files from the
original ProseMirror packages. They are checked into this repository so that CI
can run them without needing to clone external repositories.

## Source

| Package | Commit | URL |
|---|---|---|
| `prosemirror-model` | `0dc1c23bf91871e8f386a3daca0dcd59c91f4474` | <https://github.com/prosemirror/prosemirror-model> |
| `prosemirror-transform` | `3f4288e51b386905531ab1334a7f99ea2bb1c82b` | <https://github.com/prosemirror/prosemirror-transform> |

## Generation

The `.js` files were generated from the upstream TypeScript sources by running
`npm run prepare` (which invokes `pm-buildhelper src/index.ts`) in each upstream
repository. The build helper compiles `test/*.ts` in-place to `test/*.js`.

## Exclusions

`test-dom.js` is intentionally omitted because DOM parsing/serialization is out
of scope for the Rust implementation.
