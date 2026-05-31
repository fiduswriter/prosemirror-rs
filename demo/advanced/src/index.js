import { EditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { schema } from "prosemirror-schema-basic";
import { exampleSetup } from "prosemirror-example-setup";
import { keymap } from "prosemirror-keymap";
import { undo, redo } from "prosemirror-history";
import {
  goToNextCell,
  tableEditing,
  tableNodes,
} from "prosemirror-tables";
import { Schema } from "prosemirror-model";

// Build a schema that extends the basic schema with table support.
const tableSchema = new Schema({
  nodes: schema.spec.nodes.append(tableNodes({ tableGroup: "block" })),
  marks: schema.spec.marks,
});

function createEditorState() {
  return EditorState.create({
    schema: tableSchema,
    plugins: [
      ...exampleSetup({ schema: tableSchema, menuBar: true }),
      tableEditing(),
      keymap({
        Tab: goToNextCell(1),
        "Shift-Tab": goToNextCell(-1),
      }),
    ],
  });
}

// Shared steps queue for simulated collaboration.
const stepsQueue = [];

function createCollaborativeView(el, label) {
  let state = createEditorState();

  const view = new EditorView(el, {
    state,
    dispatchTransaction(tr) {
      const newState = view.state.apply(tr);
      view.updateState(newState);

      // Broadcast steps to the other editor.
      if (tr.steps.length) {
        stepsQueue.push({
          from: label,
          steps: tr.steps,
          clientID: label,
        });
      }
      updateStatus();
    },
  });

  return { view, label };
}

const editorA = createCollaborativeView(document.querySelector("#editor-a"), "A");
const editorB = createCollaborativeView(document.querySelector("#editor-b"), "B");

const editors = [editorA, editorB];

// Process queued steps every 200ms to simulate network latency.
setInterval(() => {
  while (stepsQueue.length) {
    const item = stepsQueue.shift();
    for (const { view, label } of editors) {
      if (label === item.from) continue;

      let tr = view.state.tr;
      let modified = false;
      for (const step of item.steps) {
        const mapped = step.map(tr.mapping);
        if (mapped && !tr.maybeStep(mapped).failed) {
          modified = true;
        }
      }
      if (modified) {
        view.updateState(view.state.apply(tr));
      }
    }
  }
}, 200);

function updateStatus() {
  const status = document.querySelector("#status");
  const aText = editorA.view.state.doc.textContent.slice(0, 80);
  const bText = editorB.view.state.doc.textContent.slice(0, 80);
  status.textContent =
    `Collaboration active — Editor A length: ${editorA.view.state.doc.content.size}, ` +
    `Editor B length: ${editorB.view.state.doc.content.size}`;
}

updateStatus();
