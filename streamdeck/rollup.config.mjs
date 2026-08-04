import commonjs from "@rollup/plugin-commonjs";
import nodeResolve from "@rollup/plugin-node-resolve";
import typescript from "@rollup/plugin-typescript";
import { defineConfig } from "rollup";

export default defineConfig({
  input: "src/plugin.ts",
  output: {
    file: "com.streamry.streamdeck.sdPlugin/bin/plugin.js",
    format: "cjs",
    sourcemap: true,
    exports: "auto",
  },
  plugins: [
    typescript(),
    nodeResolve({ preferBuiltins: true }),
    commonjs(),
  ],
  external: ["ws"],
});
