#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");

const platformPackage = (() => {
  if (process.platform !== "darwin") {
    return null;
  }

  if (process.arch === "arm64") {
    return "@openkeyring/ok-darwin-arm64/bin/ok";
  }

  if (process.arch === "x64") {
    return "@openkeyring/ok-darwin-x64/bin/ok";
  }

  return null;
})();

if (!platformPackage) {
  console.error(`Unsupported platform: ${process.platform}/${process.arch}`);
  process.exit(1);
}

let okBinary;
try {
  okBinary = require.resolve(platformPackage);
} catch (error) {
  console.error(`Unable to find bundled ok binary package for ${process.platform}/${process.arch}.`);
  console.error("Reinstall @openkeyring/ok and ensure optional dependencies are enabled.");
  process.exit(1);
}

const child = spawn(okBinary, process.argv.slice(2), {
  stdio: "inherit"
});

child.on("error", (error) => {
  console.error(`Failed to start ok: ${error.message}`);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});
