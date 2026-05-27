#!/usr/bin/env python3
"""
Concurrency test for prosemirror_rs under free-threaded CPython (3.13t+).

Background
----------
Without the GIL, multiple Python threads can call methods on the same
Editor instance simultaneously.  PyO3's #[pyclass] uses a per-object
RefCell to guard the Rust value.  When two threads race, the losing thread
receives RuntimeError("Already borrowed" / "Already mutably borrowed")
instead of a data race.  This is *correct and safe* behaviour — PyO3
raises rather than corrupts.

What this test verifies
-----------------------
1. RuntimeError from borrow contention is the *only* exception that can
   arise; no panics, no other errors, no data corruption.
2. After all concurrent reads and writes finish, the document is still
   well-formed JSON with the expected root type.

Run manually with a free-threaded build:
    python3.13t python/tests/test_free_threaded.py
"""

import json
import sys
import threading

from prosemirror_rs import Editor

# ── helpers ───────────────────────────────────────────────────────────────────

SCHEMA = json.dumps(
    {
        "nodes": {
            "doc": {"content": "paragraph+"},
            "paragraph": {"content": "text*", "group": "block"},
            "text": {"group": "inline"},
        },
        "marks": {"strong": {}, "em": {}},
    }
)

DOC = json.dumps(
    {
        "type": "doc",
        "content": [
            {
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "hello"},
                ],
            }
        ],
    }
)

# Insert "x" between position 2 and 2 (inside the first paragraph).
# Stays valid across repeated inserts because the paragraph keeps growing.
STEP = json.dumps(
    {
        "stepType": "replace",
        "from": 2,
        "to": 2,
        "slice": {"content": [{"type": "text", "text": "x"}]},
    }
)

N_THREADS = 20  # 10 readers + 10 writers
N_OPS = 20  # operations per thread


def _check_gil():
    """Print a clear message about whether true concurrency is in play."""
    if not hasattr(sys, "_is_gil_enabled"):
        print("sys._is_gil_enabled not available (Python < 3.12) – skipping GIL check")
        return
    if sys._is_gil_enabled():
        print(
            "WARNING: GIL is currently enabled.\n"
            "         The test will still pass, but it does not exercise true\n"
            "         parallelism.  Use a free-threaded build (python3.13t) to\n"
            "         validate concurrent safety."
        )
    else:
        print("GIL is DISABLED – exercising true parallelism.")


# ── test ──────────────────────────────────────────────────────────────────────


def test_concurrent_access():
    editor = Editor(SCHEMA, DOC)
    barrier = threading.Barrier(N_THREADS)
    errors = []
    err_lock = threading.Lock()

    def read_worker():
        """
        Calls doc_json() repeatedly.  Under free-threading, a concurrent
        apply_step() may hold the exclusive borrow, in which case PyO3
        raises RuntimeError("Already borrowed").  We catch that per-call
        so the thread keeps running; any *other* exception is a real failure.
        """
        barrier.wait()
        try:
            for _ in range(N_OPS):
                try:
                    raw = editor.doc_json()
                    doc = json.loads(raw)
                    assert doc["type"] == "doc", f"root type corrupted: got {doc['type']!r}"
                except RuntimeError:
                    # Borrow contention — expected under free-threaded Python.
                    pass
        except Exception as exc:
            with err_lock:
                errors.append(exc)

    def write_worker():
        """
        Calls apply_step() repeatedly.  A concurrent read or write may hold
        a conflicting borrow, causing RuntimeError("Already mutably borrowed"
        / "Already borrowed").  We catch that per-call; any other exception
        is a real failure.
        """
        barrier.wait()
        try:
            for _ in range(N_OPS):
                try:
                    # Return value (True/False) intentionally ignored:
                    # the step may also fail to apply if positions shifted.
                    editor.apply_step(STEP)
                except RuntimeError:
                    # Borrow contention — expected under free-threaded Python.
                    pass
        except Exception as exc:
            with err_lock:
                errors.append(exc)

    threads = [
        threading.Thread(target=read_worker, name=f"reader-{i}") for i in range(N_THREADS // 2)
    ] + [threading.Thread(target=write_worker, name=f"writer-{i}") for i in range(N_THREADS // 2)]

    for t in threads:
        t.start()
    for t in threads:
        t.join()

    if errors:
        raise AssertionError(
            f"{len(errors)} error(s) from concurrent threads:\n"
            + "\n".join(f"  {type(e).__name__}: {e}" for e in errors)
        )

    # Final document must still be well-formed.
    final_raw = editor.doc_json()
    final = json.loads(final_raw)
    assert final["type"] == "doc", "final document root type corrupted"
    assert isinstance(final["content"], list), "final document content missing"

    print(
        f"PASS  threads={N_THREADS}  ops_per_thread={N_OPS}"
        f"  version={editor.version}  doc_size={len(final_raw)}B"
    )


if __name__ == "__main__":
    _check_gil()
    test_concurrent_access()
