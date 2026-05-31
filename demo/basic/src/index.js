import { EditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { schema } from "prosemirror-schema-basic";
import { exampleSetup } from "prosemirror-example-setup";

// The bundler (rspack) is configured to alias prosemirror-model and
// prosemirror-transform to prosemirror-rs, so all heavy lifting runs
// in Rust-compiled WebAssembly.

const state = EditorState.create({
  schema,
  plugins: exampleSetup({ schema }),
});

const view = new EditorView(document.querySelector("#editor"), {
  state,
  dispatchTransaction(tr) {
    const newState = view.state.apply(tr);
    view.updateState(newState);
    updateContentPreview(newState);
  },
});

function updateContentPreview(state) {
  document.querySelector("#content").innerHTML =
    "<strong>Document JSON:</strong><pre>" +
    JSON.stringify(state.doc.toJSON(), null, 2) +
    "</pre>";
}

updateContentPreview(state);
