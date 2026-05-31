const original = require("ist");

function ist(a, b, compare) {
  if (arguments.length === 1) return original(a);
  if (
    !compare &&
    a &&
    b &&
    typeof a.eq === "function" &&
    typeof b === "object"
  ) {
    if (!a.eq(b)) throw new ist.Failure(a + " != " + b, "ist");
    return;
  }
  return original(a, b, compare);
}
ist.Failure = original.Failure;

// WASM throws plain strings, not Error objects. Wrap .throws to handle both.
ist.throws = function throws(f, expected) {
  let threw = true;
  try {
    f();
    threw = false;
  } catch (e) {
    // WASM throws strings — promote to Error so .message and .test work
    if (typeof e === 'string') e = new Error(e);
    var matches = !expected ? true
        : expected.test ? expected.test(e.message)
        : typeof expected == 'string' ? e.message == expected
        : typeof expected === 'function' ? expected(e)
        : false;
    if (!matches) throw e;
  }
  if (!threw) throw new ist.Failure("Did not throw", "throws");
};

module.exports = ist;
