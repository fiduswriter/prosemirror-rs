import { EditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { schema } from "prosemirror-schema-basic";
import { exampleSetup } from "prosemirror-example-setup";
import * as model from "prosemirror-model";
import * as transform from "prosemirror-transform";

window._model = model;
window._transform = transform;

// The bundler (rspack) is configured to alias prosemirror-model and
// prosemirror-transform to prosemirror-rs, so all heavy lifting runs
// in Rust-compiled WebAssembly.

const state = EditorState.create({
  schema,
  plugins: exampleSetup({ schema }),
});

const view = new EditorView(document.querySelector("#editor"), {
  state,
});

// Expose for tests
window._proseMirrorView = view;
window._proseMirrorState = state;
