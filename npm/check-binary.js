#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const binary = path.join(process.cwd(), "bin", "ok");

try {
  fs.accessSync(binary, fs.constants.X_OK);
} catch {
  console.error(`Missing executable binary: ${binary}`);
  console.error("Run npm/stage-binaries.sh before packing or publishing this platform package.");
  process.exit(1);
}
