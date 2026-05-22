#!/usr/bin/env node
// Thin spawn shim: exec the pre-built native binary in ../vendor/ with the
// caller's argv and inherit stdio. Exit with the child's exit code.

"use strict";

const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const binName = process.platform === "win32" ? "drawio-headless.exe" : "drawio-headless";
const binPath = path.join(__dirname, "..", "vendor", binName);

if (!fs.existsSync(binPath)) {
  console.error(
    `drawio-headless: native binary not found at ${binPath}. The postinstall ` +
      `step may have failed — re-run \`npm install drawio-headless\` or see ` +
      `https://github.com/mvhenten/drawio-headless#install for alternatives.`
  );
  process.exit(127);
}

const result = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`drawio-headless: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
