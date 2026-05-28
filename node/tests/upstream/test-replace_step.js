import ist from "ist";
import { Slice, Fragment } from "prosemirror-model";
import { doc, blockquote, p, schema, eq } from "prosemirror-test-builder";
import { Transform, ReplaceAroundStep } from "prosemirror-transform";
describe("ReplaceAroundStep", () => {
    it("verifies that the inserted content fits", () => {
        let tr = new Transform(doc(p("a")));
        ist.throws(() => {
            tr.step(new ReplaceAroundStep(1, 2, 1, 2, new Slice(Fragment.from(blockquote()), 0, 0), 1, true));
        }, /Content does not fit in gap/);
    });
    it("considers slice openness when verifying content fit", () => {
        let tr = new Transform(doc(blockquote(p("x"))));
        tr.step(new ReplaceAroundStep(0, 1, 1, 1, new Slice(Fragment.from(blockquote()), 0, 1), 1, true));
        ist(tr.doc, doc(blockquote(p("x"))), eq);
    });
    describe("map", () => {
        function test(doc, change, otherChange, expected) {
            let trA = new Transform(doc), trB = new Transform(doc);
            change(trA);
            otherChange(trB);
            let result = new Transform(trB.doc).step(trA.steps[0].map(trB.mapping)).doc;
            ist(result, expected, eq);
        }
        it("doesn't break wrap steps on insertions", () => test(doc(p("a")), tr => tr.wrap(tr.doc.resolve(1).blockRange(), [{ type: schema.nodes.blockquote }]), tr => tr.insert(0, p("b")), doc(p("b"), blockquote(p("a")))));
        it("doesn't overwrite content inserted at start of unwrap step", () => test(doc(blockquote(p("a"))), tr => tr.lift(tr.doc.resolve(2).blockRange(), 0), tr => tr.insert(2, schema.text("x")), doc(p("xa"))));
    });
});
