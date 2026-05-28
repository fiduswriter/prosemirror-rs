const original = require("ist");

function ist(a, b, compare) {
  if (arguments.length === 1) return original(a);
  if (!compare && a && b && typeof a.eq === "function") {
    if (!a.eq(b)) throw new ist.Failure(a + " != " + b, "ist");
    return;
  }
  return original(a, b, compare);
}
ist.Failure = original.Failure;
ist.throws = original.throws;
module.exports = ist;
