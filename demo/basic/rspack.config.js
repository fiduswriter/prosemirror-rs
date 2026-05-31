import { dirname, resolve } from "path";
import { fileURLToPath } from "url";
import { CopyRspackPlugin } from "@rspack/core";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default {
  entry: "./src/index.js",
  output: {
    path: resolve(__dirname, "dist"),
    filename: "bundle.js",
    publicPath: "auto",
  },
  resolve: {
    alias: {
      "prosemirror-model": "prosemirror-rs",
      "prosemirror-transform": "prosemirror-rs",
    },
  },
  experiments: {
    outputModule: true,
  },
  plugins: [
    new CopyRspackPlugin({
      patterns: [
        { from: resolve(__dirname, "public"), to: "." },
        {
          from: resolve(__dirname, "node_modules/prosemirror-rs/wasm/prosemirror_rs_wasm_bg.wasm"),
          to: ".",
        },
      ],
    }),
  ],
};
