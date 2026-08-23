import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appName = "mediafilez-desktop";
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

const paths = {
  packageJson: resolve(projectRoot, "package.json"),
  tauriConfig: resolve(projectRoot, "src-tauri", "tauri.conf.json"),
  cargoManifest: resolve(projectRoot, "src-tauri", "Cargo.toml"),
  cargoLock: resolve(projectRoot, "src-tauri", "Cargo.lock"),
};

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function readCargoManifestVersion(source) {
  const packageStart = source.indexOf("[package]");
  const packageEnd = source.indexOf("\n[", packageStart + 1);
  const section = source.slice(packageStart, packageEnd === -1 ? undefined : packageEnd);
  const match = section.match(/^version\s*=\s*"([^"]+)"/m);

  if (packageStart === -1 || !match) {
    throw new Error("Could not read [package].version from src-tauri/Cargo.toml.");
  }

  return match[1];
}

function writeCargoManifestVersion(source, version) {
  const packageStart = source.indexOf("[package]");
  const packageEnd = source.indexOf("\n[", packageStart + 1);
  const end = packageEnd === -1 ? source.length : packageEnd;
  const section = source.slice(packageStart, end);

  if (packageStart === -1 || !/^version\s*=\s*"[^"]+"/m.test(section)) {
    throw new Error("Could not update [package].version in src-tauri/Cargo.toml.");
  }

  const updated = section.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
  return source.slice(0, packageStart) + updated + source.slice(end);
}

function findLockPackage(source) {
  return source
    .split(/(?=\[\[package\]\]\r?\n)/)
    .find((block) => new RegExp(`^name\\s*=\\s*"${appName}"$`, "m").test(block));
}

function readCargoLockVersion(source) {
  const block = findLockPackage(source);
  const match = block?.match(/^version\s*=\s*"([^"]+)"/m);

  if (!match) {
    throw new Error(`Could not read ${appName} version from src-tauri/Cargo.lock.`);
  }

  return match[1];
}

function writeCargoLockVersion(source, version) {
  let found = false;
  const blocks = source.split(/(?=\[\[package\]\]\r?\n)/);
  const updated = blocks.map((block) => {
    if (!new RegExp(`^name\\s*=\\s*"${appName}"$`, "m").test(block)) {
      return block;
    }

    found = true;
    return block.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
  });

  if (!found) {
    throw new Error(`Could not update ${appName} version in src-tauri/Cargo.lock.`);
  }

  return updated.join("");
}

function readVersions() {
  const cargoManifest = readFileSync(paths.cargoManifest, "utf8");
  const cargoLock = readFileSync(paths.cargoLock, "utf8");

  return {
    "package.json": readJson(paths.packageJson).version,
    "src-tauri/tauri.conf.json": readJson(paths.tauriConfig).version,
    "src-tauri/Cargo.toml": readCargoManifestVersion(cargoManifest),
    "src-tauri/Cargo.lock": readCargoLockVersion(cargoLock),
  };
}

function checkVersions() {
  const versions = readVersions();
  const uniqueVersions = new Set(Object.values(versions));

  if (uniqueVersions.size !== 1) {
    const details = Object.entries(versions)
      .map(([file, version]) => `${file}: ${version}`)
      .join("\n");
    throw new Error(`Project versions do not match:\n${details}`);
  }

  const [version] = uniqueVersions;
  if (!semverPattern.test(version)) {
    throw new Error(`Invalid semantic version: ${version}`);
  }

  if (process.env.GITHUB_REF_TYPE === "tag") {
    const expectedTag = `v${version}`;
    if (process.env.GITHUB_REF_NAME !== expectedTag) {
      throw new Error(`Release tag ${process.env.GITHUB_REF_NAME} does not match ${expectedTag}.`);
    }
  }

  console.log(`Version ${version} is consistent across the project.`);
  return version;
}

function setVersion(version) {
  if (!semverPattern.test(version ?? "")) {
    throw new Error("Usage: pnpm version:set <semver>, for example pnpm version:set 0.2.0");
  }

  const packageJson = readJson(paths.packageJson);
  const tauriConfig = readJson(paths.tauriConfig);
  const cargoManifest = writeCargoManifestVersion(
    readFileSync(paths.cargoManifest, "utf8"),
    version,
  );
  const cargoLock = writeCargoLockVersion(readFileSync(paths.cargoLock, "utf8"), version);
  packageJson.version = version;
  tauriConfig.version = version;

  writeJson(paths.packageJson, packageJson);
  writeJson(paths.tauriConfig, tauriConfig);
  writeFileSync(paths.cargoManifest, cargoManifest);
  writeFileSync(paths.cargoLock, cargoLock);

  checkVersions();
}

try {
  const [command = "check", value] = process.argv.slice(2);

  if (command === "check") {
    checkVersions();
  } else if (command === "set") {
    setVersion(value);
  } else {
    throw new Error(`Unknown version command: ${command}`);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
