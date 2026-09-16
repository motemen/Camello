// Finding a server to talk to.
//
// The extension stays thin, and this is the one thing it does that the server
// does not: when there is no `camello` to spawn, fetch the one this extension
// was built against. The binaries are the release assets the `v*` tag already
// publishes, so there is nothing here to keep in step with the server — the
// version in `package.json` names the tag, and the tag names the asset.
//
// Nothing is fetched silently over a working binary: `camello.path` wins, then
// whatever is on `PATH`, and only a machine with neither downloads anything.

import * as vscode from "vscode";
import * as crypto from "node:crypto";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);

const REPOSITORY = "motemen/Camello";

/// The release assets are named by target triple; these are the four the
/// release workflow builds. A platform not on this list has no binary to fetch,
/// and says so rather than downloading something that will not run.
function target(): string | undefined {
  const targets: Record<string, string> = {
    "darwin-arm64": "aarch64-apple-darwin",
    "darwin-x64": "x86_64-apple-darwin",
    "linux-arm64": "aarch64-unknown-linux-musl",
    "linux-x64": "x86_64-unknown-linux-musl",
  };
  return targets[`${process.platform}-${process.arch}`];
}

/// Where to spawn the server from, fetching it first if there is no other.
///
/// Returns the command to run. The three answers are in order of how much the
/// user has said about it: a configured path is taken as given and never
/// second-guessed, a `camello` on `PATH` is the ordinary install, and the
/// downloaded copy is the fallback for neither.
export async function serverCommand(
  context: vscode.ExtensionContext,
): Promise<string> {
  const configured = vscode.workspace
    .getConfiguration("camello")
    .get<string>("path");
  if (configured && configured.length > 0) {
    return configured;
  }
  if (await onPath("camello")) {
    return "camello";
  }
  return await download(context);
}

async function onPath(command: string): Promise<boolean> {
  try {
    await run(command, ["--version"]);
    return true;
  } catch {
    return false;
  }
}

/// The downloaded server for this extension's version, fetched once.
///
/// Keyed by version rather than overwritten, so an extension update fetches its
/// own server and a downgrade finds the one it left behind.
async function download(context: vscode.ExtensionContext): Promise<string> {
  const version = context.extension.packageJSON.version as string;
  const triple = target();
  if (!triple) {
    throw new Error(
      `no released binary for ${process.platform}-${process.arch}. ` +
        "Build one with `cargo install --path .` and set `camello.path`.",
    );
  }

  const root = context.globalStorageUri.fsPath;
  const directory = path.join(root, `camello-${version}-${triple}`);
  const binary = path.join(directory, "camello");
  if (await exists(binary)) {
    return binary;
  }

  const name = `camello-${version}-${triple}.tar.gz`;
  const base = `https://github.com/${REPOSITORY}/releases/download/v${version}`;

  return await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Camello: downloading the ${version} server`,
      cancellable: false,
    },
    async () => {
      await fs.mkdir(root, { recursive: true });
      const scratch = await fs.mkdtemp(path.join(root, "download-"));
      try {
        const archive = path.join(scratch, name);
        await fetchTo(`${base}/${name}`, archive);
        await verify(archive, `${base}/${name}.sha256`);

        // `tar` rather than a library: it is on every platform this fetches for,
        // and a dependency that unpacks archives is a dependency that has to be
        // audited.
        await run("tar", ["-xzf", archive, "-C", scratch]);

        // The tarball holds one directory named after the archive, and the
        // binary sits in it beside the README and the changelog.
        const unpacked = path.join(scratch, `camello-${version}-${triple}`);
        await fs.rm(directory, { recursive: true, force: true });
        await fs.rename(unpacked, directory);
        await fs.chmod(binary, 0o755);
        return binary;
      } finally {
        await fs.rm(scratch, { recursive: true, force: true });
      }
    },
  );
}

async function fetchTo(url: string, destination: string): Promise<void> {
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) {
    throw new Error(`GET ${url} — ${response.status} ${response.statusText}`);
  }
  await fs.writeFile(destination, Buffer.from(await response.arrayBuffer()));
}

/// Check the archive against the `.sha256` published beside it.
///
/// The file is `shasum -a 256` output — the digest, then the name it was taken
/// of — and only the digest is of interest here; the name is the one we asked
/// for by construction.
async function verify(archive: string, url: string): Promise<void> {
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) {
    throw new Error(`GET ${url} — ${response.status} ${response.statusText}`);
  }
  const expected = (await response.text()).trim().split(/\s+/)[0];
  const actual = crypto
    .createHash("sha256")
    .update(await fs.readFile(archive))
    .digest("hex");
  if (expected !== actual) {
    throw new Error(
      `checksum mismatch for ${path.basename(archive)}: ` +
        `expected ${expected}, got ${actual}`,
    );
  }
}

async function exists(file: string): Promise<boolean> {
  try {
    await fs.access(file);
    return true;
  } catch {
    return false;
  }
}
