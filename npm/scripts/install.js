#!/usr/bin/env node
// Postinstall: download the matching pre-built `drawio-headless` binary from
// GitHub Releases into `vendor/` and mark it executable.
//
// No npm dependencies on purpose. Uses only Node stdlib and shells out to
// `tar` (unix) or PowerShell `Expand-Archive` (Windows) for extraction.
//
// Skipped when `DRAWIO_HEADLESS_SKIP_DOWNLOAD=1` is set — useful for local
// development against a `cargo`-built binary.

"use strict";

const fs = require("fs");
const path = require("path");
const https = require("https");
const { spawnSync } = require("child_process");

const REPO = "mvhenten/drawio-headless";
const PKG = require(path.join(__dirname, "..", "package.json"));
const VERSION = PKG.version;

const PLATFORM_MAP = {
  "linux-x64": { target: "x86_64-unknown-linux-gnu", archive: "tar.gz", bin: "drawio-headless" },
  "linux-arm64": { target: "aarch64-unknown-linux-gnu", archive: "tar.gz", bin: "drawio-headless" },
  "darwin-x64": { target: "x86_64-apple-darwin", archive: "tar.gz", bin: "drawio-headless" },
  "darwin-arm64": { target: "aarch64-apple-darwin", archive: "tar.gz", bin: "drawio-headless" },
  "win32-x64": { target: "x86_64-pc-windows-msvc", archive: "zip", bin: "drawio-headless.exe" },
};

function detectTarget() {
  const key = `${process.platform}-${process.arch}`;
  const entry = PLATFORM_MAP[key];
  if (!entry) {
    throw new Error(
      `Unsupported platform/arch: ${key}. Supported: ${Object.keys(PLATFORM_MAP).join(", ")}.\n` +
        `Build from source instead: cargo install --git https://github.com/${REPO} --path crates/cli`
    );
  }
  return entry;
}

function downloadFollowingRedirects(url, dest, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, { headers: { "User-Agent": "drawio-headless-npm-installer" } }, (res) => {
      const status = res.statusCode || 0;
      if (status >= 300 && status < 400 && res.headers.location) {
        res.resume();
        if (redirectsLeft <= 0) {
          reject(new Error(`Too many redirects fetching ${url}`));
          return;
        }
        const nextUrl = new URL(res.headers.location, url).toString();
        downloadFollowingRedirects(nextUrl, dest, redirectsLeft - 1).then(resolve, reject);
        return;
      }
      if (status !== 200) {
        res.resume();
        reject(new Error(`Download failed: ${url} -> HTTP ${status}`));
        return;
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close((err) => (err ? reject(err) : resolve())));
      file.on("error", reject);
    });
    request.on("error", reject);
    request.setTimeout(60_000, () => {
      request.destroy(new Error(`Download timed out: ${url}`));
    });
  });
}

function extract(archivePath, destDir, isZip) {
  fs.mkdirSync(destDir, { recursive: true });
  if (isZip) {
    // PowerShell is present on all supported Windows runners. Use -Force to
    // overwrite any prior install.
    const result = spawnSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Expand-Archive -Path '${archivePath}' -DestinationPath '${destDir}' -Force`,
      ],
      { stdio: "inherit" }
    );
    if (result.status !== 0) {
      throw new Error(`Expand-Archive failed (exit ${result.status})`);
    }
  } else {
    const result = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], { stdio: "inherit" });
    if (result.status !== 0) {
      throw new Error(`tar extraction failed (exit ${result.status})`);
    }
  }
}

function findBinary(rootDir, binName) {
  // The archive layout from release.yml is:
  //   drawio-headless-<version>-<target>/drawio-headless[.exe]
  const entries = fs.readdirSync(rootDir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(rootDir, entry.name);
    if (entry.isDirectory()) {
      const candidate = path.join(full, binName);
      if (fs.existsSync(candidate)) return candidate;
    }
    if (entry.isFile() && entry.name === binName) return full;
  }
  return null;
}

async function main() {
  if (process.env.DRAWIO_HEADLESS_SKIP_DOWNLOAD === "1") {
    console.log("[drawio-headless] DRAWIO_HEADLESS_SKIP_DOWNLOAD=1 — skipping binary download.");
    return;
  }

  const target = detectTarget();
  const vendorDir = path.join(__dirname, "..", "vendor");
  const finalBin = path.join(vendorDir, target.bin);

  if (fs.existsSync(finalBin)) {
    console.log(`[drawio-headless] Binary already present at ${finalBin}, skipping download.`);
    return;
  }

  fs.mkdirSync(vendorDir, { recursive: true });

  const assetName = `drawio-headless-${VERSION}-${target.target}.${target.archive}`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${assetName}`;
  const archivePath = path.join(vendorDir, assetName);

  console.log(`[drawio-headless] Downloading ${url}`);
  try {
    await downloadFollowingRedirects(url, archivePath);
  } catch (err) {
    fs.rmSync(archivePath, { force: true });
    console.error(`[drawio-headless] Download failed: ${err.message}`);
    console.error(
      `[drawio-headless] You can install manually from https://github.com/${REPO}/releases/tag/v${VERSION}` +
        ` or build from source: cargo install --git https://github.com/${REPO} --path crates/cli`
    );
    process.exit(1);
  }

  const extractDir = path.join(vendorDir, "_extract");
  fs.rmSync(extractDir, { recursive: true, force: true });
  try {
    extract(archivePath, extractDir, target.archive === "zip");
  } catch (err) {
    console.error(`[drawio-headless] Extraction failed: ${err.message}`);
    process.exit(1);
  }

  const extracted = findBinary(extractDir, target.bin);
  if (!extracted) {
    console.error(`[drawio-headless] Binary ${target.bin} not found in extracted archive.`);
    process.exit(1);
  }

  fs.copyFileSync(extracted, finalBin);
  fs.chmodSync(finalBin, 0o755);

  // Clean up: keep only the final binary in vendor/, drop archive + extract dir.
  fs.rmSync(extractDir, { recursive: true, force: true });
  fs.rmSync(archivePath, { force: true });

  console.log(`[drawio-headless] Installed binary at ${finalBin}`);
}

main().catch((err) => {
  console.error(`[drawio-headless] Postinstall failed: ${err.stack || err.message}`);
  process.exit(1);
});
