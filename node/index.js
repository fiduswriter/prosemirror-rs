"use strict";

const { existsSync } = require("fs");
const { join } = require("path");

const { platform, arch } = process;

// Map Node.js platform + arch → napi-rs platform triple.
// Every triple listed here must have a corresponding build in the CI matrix.
const TRIPLES = {
  "darwin-x64": "prosemirror-rs.darwin-x64.node",
  "darwin-arm64": "prosemirror-rs.darwin-arm64.node",
  "linux-x64": "prosemirror-rs.linux-x64-gnu.node",
  "linux-arm64": "prosemirror-rs.linux-arm64-gnu.node",
  "win32-x64": "prosemirror-rs.win32-x64-msvc.node",
};

const key = `${platform}-${arch}`;
const filename = TRIPLES[key];

if (!filename) {
  throw new Error(
    `Unsupported platform/architecture: ${key}. ` +
      `prosemirror-rs distributes prebuilt binaries for ` +
      `darwin-x64, darwin-arm64, linux-x64, linux-arm64, and win32-x64.`,
  );
}

const localPath = join(__dirname, filename);
if (!existsSync(localPath)) {
  throw new Error(
    `No native binary found for ${key} at ${filename}. ` +
      `Try reinstalling the package, or build from source with \`npm run build\`.`,
  );
}

module.exports = require(localPath);
