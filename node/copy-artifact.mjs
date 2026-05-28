/**
 * Copies the compiled Rust native addon to a platform-specific .node file.
 *
 * The filename follows the napi-rs triple convention so that index.js can
 * load the correct binary at runtime.  Called automatically by `npm run build`
 * after `cargo build --release`.
 *
 * Environment variables (optional, used in CI):
 *   RUST_TARGET  – cross-compilation target, e.g. "aarch64-unknown-linux-gnu"
 *                  When set, the binary is looked up under
 *                  target/<RUST_TARGET>/release/ instead of target/release/.
 */
import { cpSync, existsSync } from "fs";
import { platform, arch } from "os";
import { fileURLToPath } from "url";
import { join, dirname } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));

// ── Per-platform helper: shared library filename (without path) ───────────
// Windows:   prosemirror_rs.dll      (no lib- prefix)
// macOS:     libprosemirror_rs.dylib
// Linux:     libprosemirror_rs.so
function libFilename() {
  const p = platform();
  if (p === "win32") return "prosemirror_rs.dll";
  if (p === "darwin") return "libprosemirror_rs.dylib";
  return "libprosemirror_rs.so";
}

// ── Build directory ──────────────────────────────────────────────────────
// When cross-compiling, cargo places output under target/<rust_target>/release/
// instead of target/release/.
function buildDir() {
  const rustTarget = process.env.RUST_TARGET;
  if (rustTarget) {
    return join(__dirname, "..", "target", rustTarget, "release");
  }
  return join(__dirname, "..", "target", "release");
}

// ── napi-rs platform triple for the output file ──────────────────────────
function triple() {
  const p = platform();
  const a = arch();
  if (p === "darwin" && a === "x64") return "darwin-x64";
  if (p === "darwin" && a === "arm64") return "darwin-arm64";
  if (p === "linux" && a === "x64") return "linux-x64-gnu";
  if (p === "linux" && a === "arm64") return "linux-arm64-gnu";
  if (p === "win32" && a === "x64") return "win32-x64-msvc";
  throw new Error(`Unsupported platform/arch: ${p}-${a}`);
}

const src = join(buildDir(), libFilename());
const dest = join(__dirname, `prosemirror-rs.${triple()}.node`);

if (!existsSync(src)) {
  console.error(`ERROR: compiled artifact not found at ${src}`);
  console.error("");
  console.error(
    "Make sure cargo build --release completed successfully first.",
  );
  if (process.env.RUST_TARGET) {
    console.error(`(RUST_TARGET=${process.env.RUST_TARGET} is set)`);
  }
  process.exit(1);
}

cpSync(src, dest);
console.log(`Copied ${src} → ${dest}`);
