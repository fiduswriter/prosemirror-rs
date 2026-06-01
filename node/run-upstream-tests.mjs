#!/usr/bin/env node
/**
 * Run upstream ProseMirror JS tests against the Rust implementation.
 *
 * Usage:
 *   node run-upstream-tests.mjs          # napi back-end (default)
 *   node run-upstream-tests.mjs --wasm   # WASM back-end
 */
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  rmSync,
  existsSync,
  readdirSync,
} from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { spawnSync, execSync } from "child_process";

const USE_WASM = process.argv.includes("--wasm");

const __dirname = dirname(fileURLToPath(import.meta.url));

// Upstream test directories (relative to this script)
const UPSTREAM_DIRS = ["tests/upstream"];

// Files to exclude (DOM-related tests — no DOM in Rust)
const EXCLUDE_FILES = new Set(["test-dom.js", "test-dom.ts"]);

// Import replacements
const SHIM_DIR = USE_WASM ? "test-shim/wasm" : "test-shim";
const NODE_PATH = USE_WASM
  ? "npm/wasm-nodejs/prosemirror_rs_wasm.js"
  : "npm/napi/prosemirror-rs.linux-x64-gnu.node";

const IMPORT_REPLACEMENTS = [
  { from: "prosemirror-model", to: `./${SHIM_DIR}/prosemirror-model.cjs` },
  {
    from: "prosemirror-transform",
    to: `./${SHIM_DIR}/prosemirror-transform.cjs`,
  },
  {
    from: "prosemirror-test-builder",
    to: "./test-shim/prosemirror-test-builder.cjs",
  },
  { from: "ist", to: "./test-shim/ist.cjs" },
];

const TEMP_DIR = join(__dirname, ".upstream-tests");

function clean() {
  if (existsSync(TEMP_DIR)) {
    rmSync(TEMP_DIR, { recursive: true });
  }
  mkdirSync(TEMP_DIR, { recursive: true });
  writeFileSync(
    join(TEMP_DIR, "package.json"),
    JSON.stringify({ type: "commonjs" }),
    "utf-8",
  );

  // Copy shim files and native binary into temp dir so relative requires work
  const shimDir = join(__dirname, "test-shim");
  const destShimDir = join(TEMP_DIR, "test-shim");
  mkdirSync(destShimDir, { recursive: true });
  for (const file of readdirSync(shimDir)) {
    const srcPath = join(shimDir, file);
    if (!existsSync(srcPath) || !srcPath.endsWith(".cjs")) continue;
    let content = readFileSync(srcPath, "utf-8");
    content = content.replace(
      /require\(['"]\.\.\/npm\/napi\/prosemirror-rs\.linux-x64-gnu\.node['"]\)/g,
      'require("./prosemirror-rs.linux-x64-gnu.node")',
    );
    // For WASM mode, redirect the test-builder's prosemirror-model require
    if (USE_WASM && file === "prosemirror-test-builder.cjs") {
      content = content.replace(
        /require\(['"]prosemirror-model['"]\)/g,
        'require("./wasm/prosemirror-model.cjs")',
      );
      content = content.replace(
        /require\(['"]\.\/prosemirror-model\.cjs['"]\)/g,
        'require("./wasm/prosemirror-model.cjs")',
      );
    }
    writeFileSync(join(destShimDir, file), content, "utf-8");
  }

  // Copy WASM shim files if they exist (no path rewriting needed)
  const wasmShimDir = join(__dirname, "test-shim", "wasm");
  if (existsSync(wasmShimDir)) {
    const destWasmShimDir = join(TEMP_DIR, "test-shim", "wasm");
    mkdirSync(destWasmShimDir, { recursive: true });
    for (const file of readdirSync(wasmShimDir)) {
      writeFileSync(
        join(destWasmShimDir, file),
        readFileSync(join(wasmShimDir, file)),
      );
    }
  }

  // Copy native binary or WASM files to temp dir
  if (USE_WASM) {
    // Copy entire wasm-nodejs directory
    const wasmNodeDir = join(__dirname, "npm", "wasm-nodejs");
    const destWasmDir = join(TEMP_DIR, "npm", "wasm-nodejs");
    mkdirSync(destWasmDir, { recursive: true });
    for (const file of readdirSync(wasmNodeDir)) {
      const src = join(wasmNodeDir, file);
      if (!existsSync(src) || src.endsWith("/")) continue;
      try {
        writeFileSync(join(destWasmDir, file), readFileSync(src));
      } catch (_) { /* skip dirs */ }
    }
    // Also copy patch.js, dom.js, and DOM helper files
    const npmDir = join(__dirname, "npm");
    for (const f of ["patch.js", "dom.js", "to-dom.js", "from-dom.js"]) {
      writeFileSync(join(TEMP_DIR, "npm", f), readFileSync(join(npmDir, f)));
    }
  } else {
    const binaryName = "prosemirror-rs.linux-x64-gnu.node";
    const binaryData = readFileSync(join(__dirname, "npm", "napi", binaryName));
    writeFileSync(join(TEMP_DIR, binaryName), binaryData);
    writeFileSync(join(destShimDir, binaryName), binaryData);
  }
}

function copyAndRewrite(srcDir, file) {
  const srcPath = join(srcDir, file);
  const content = readFileSync(srcPath, "utf-8");

  // Replace ES module imports with CommonJS requires
  let rewritten = content;

  // Handle: import {X, Y} from "module"  (including aliases like "schema as baseSchema")
  rewritten = rewritten.replace(
    /import\s+\{([^}]+)\}\s+from\s+"([^"]+)"/g,
    (match, bindings, module) => {
      const replacement = IMPORT_REPLACEMENTS.find((r) => r.from === module);
      if (replacement) {
        // Convert "schema as baseSchema" to "schema: baseSchema"
        const converted = bindings
          .split(",")
          .map((b) => {
            const m = b.trim().match(/^(\w+)\s+as\s+(\w+)$/);
            if (m) return `${m[1]}: ${m[2]}`;
            return b.trim();
          })
          .join(", ");
        return `const {${converted}} = require("${replacement.to}")`;
      }
      return match;
    },
  );

  // Handle: import X from "module"
  rewritten = rewritten.replace(
    /import\s+(\w+)\s+from\s+"([^"]+)"/g,
    (match, binding, module) => {
      const replacement = IMPORT_REPLACEMENTS.find((r) => r.from === module);
      if (replacement) {
        return `const ${binding} = require("${replacement.to}")`;
      }
      return match;
    },
  );

  // Handle relative imports like: import {testTransform} from "./trans.js"
  rewritten = rewritten.replace(
    /import\s+\{([^}]+)\}\s+from\s+"\.\/([^"]+)"/g,
    (match, bindings, path) => {
      const cjsPath = path.replace(/\.js$/, ".cjs");
      return `const {${bindings}} = require("./${cjsPath}")`;
    },
  );
  rewritten = rewritten.replace(
    /import\s+(\w+)\s+from\s+"\.\/([^"]+)"/g,
    (match, binding, path) => {
      const cjsPath = path.replace(/\.js$/, ".cjs");
      return `const ${binding} = require("./${cjsPath}")`;
    },
  );

  // Replace any remaining import statements
  rewritten = rewritten.replace(/^import\s+.*from\s+".*";?\s*$/gm, "");
  rewritten = rewritten.replace(/^import\s+.*;?\s*$/gm, "");

  // Handle ES exports in helper files like trans.js
  const exports = [];
  rewritten = rewritten.replace(/export\s+function\s+(\w+)/g, (match, name) => {
    exports.push(name);
    return `function ${name}`;
  });
  rewritten = rewritten.replace(/export\s+class\s+(\w+)/g, (match, name) => {
    exports.push(name);
    return `class ${name}`;
  });
  rewritten = rewritten.replace(
    /export\s+default\s+(\w+);?/g,
    (match, name) => {
      exports.push(`default: ${name}`);
      return "";
    },
  );
  rewritten = rewritten.replace(
    /export\s*\{([^}]+)\}\s*;?/g,
    (match, names) => {
      names.split(",").forEach((n) => exports.push(n.trim()));
      return "";
    },
  );
  if (exports.length > 0) {
    const defaultExport = exports.find((e) => e.startsWith("default:"));
    const namedExports = exports.filter((e) => !e.startsWith("default:"));
    if (defaultExport && namedExports.length === 0) {
      rewritten += `\nmodule.exports = ${defaultExport.slice(9)};\n`;
    } else {
      const pairs = namedExports.map((n) => `${n}: ${n}`).join(", ");
      if (defaultExport) {
        rewritten += `\nmodule.exports = Object.assign(${defaultExport.slice(9)}, { ${pairs} });\n`;
      } else {
        rewritten += `\nmodule.exports = { ${pairs} };\n`;
      }
    }
  }

  // Replace dynamic import with require
  rewritten = rewritten.replace(/import\("fs"\)/g, 'require("fs")');

  const destPath = join(TEMP_DIR, file.replace(/\.js$/, ".cjs"));
  writeFileSync(destPath, rewritten, "utf-8");
  return destPath;
}

function main() {
  clean();

  const testFiles = [];

  for (const relDir of UPSTREAM_DIRS) {
    const absDir = join(__dirname, relDir);
    const files = readdirSync(absDir);
    for (const file of files) {
      if (!file.endsWith(".js")) continue;
      if (EXCLUDE_FILES.has(file)) {
        console.log(`Skipping excluded file: ${file}`);
        continue;
      }
      const destPath = copyAndRewrite(absDir, file);
      testFiles.push(destPath);
      console.log(`Prepared: ${file}`);
    }
  }

  if (testFiles.length === 0) {
    console.error("No test files found!");
    process.exit(1);
  }

  console.log(`\nRunning ${testFiles.length} test files with Mocha...\n`);

  // Use explicit file list (no shell glob)
  const result = spawnSync(process.execPath, [
    join(__dirname, "node_modules", ".bin", "mocha"),
    ...testFiles,
  ], {
    cwd: __dirname,
    stdio: "inherit",
  });

  if (result.signal) {
    console.error(`\nTest process terminated by signal: ${result.signal}`);
    process.exit(1);
  }
  process.exit(result.status ?? 0);
}

main();
