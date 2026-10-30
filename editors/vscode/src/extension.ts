// The thin half of `camello lsp` (docs/lsp.md, "The VS Code extension").
//
// Find the server, spawn `camello lsp`, hand the connection to
// vscode-languageclient, and get out of the way. Everything a user can
// configure here is a pass-through of the server's own configuration —
// camello.toml is read by the server, not by this file — so an eglot or
// nvim-lspconfig user who points at `camello lsp` themselves gets the
// identical server.
//
// Two things are the extension's own, and both are about reaching the server
// rather than about answering for it: fetching a binary when the machine has
// none (`bootstrap.ts`), and running `camello check` over the whole tree, which
// the server does not do by design (`check.ts`).

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import { serverCommand } from "./bootstrap";
import * as check from "./check";

let client: LanguageClient | undefined;
// The command the running server was spawned from, so the workspace check runs
// the same binary the editor is being answered by rather than resolving its own.
let spawnedFrom = "camello";
// One for the extension's lifetime, not one per server: a restart that made a
// new one would leave another "Camello" in the Output panel's list each time.
let outputChannel: vscode.OutputChannel | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  outputChannel = vscode.window.createOutputChannel("Camello");
  context.subscriptions.push(
    outputChannel,
    vscode.commands.registerCommand("camello.restartServer", async () => {
      await stop();
      await start(context);
    }),
  );
  check.register(context, () => spawnedFrom);
  await start(context);
}

export async function deactivate(): Promise<void> {
  await stop();
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  let command: string;
  try {
    command = await serverCommand(context);
  } catch (error) {
    vscode.window.showErrorMessage(`camello: no server to run — ${error}.`);
    return;
  }
  spawnedFrom = command;
  // No `transport`: for an Executable that is not a default but a flag —
  // vscode-languageclient appends `--stdio` to the arguments, and `camello
  // lsp` has no such flag. Left out, the client spawns the same process and
  // talks over the same pipes, which is what `camello lsp` already speaks.
  const server: ServerOptions = {
    run: { command, args: ["lsp"] },
    debug: { command, args: ["lsp"] },
  };

  // No `synchronize.fileEvents`: the server asks for the files it watches
  // with a dynamic registration at startup, and vscode-languageclient sets up
  // that watcher itself. A second one here would send every event twice.
  const options: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "perl" }],
    outputChannel,
  };

  client = new LanguageClient("camello", "Camello", server, options);
  try {
    await client.start();
  } catch (error) {
    vscode.window.showErrorMessage(
      `camello: could not start \`${command} lsp\` — ${error}. ` +
        "Set `camello.path` if the binary is not on your PATH.",
    );
    client = undefined;
    return;
  }
  // Not pushed to `context.subscriptions`: `stop` disposes of the client, on a
  // restart and from `deactivate`, and a list that held every client ever
  // started would keep each stopped one alive until the window closed.

  // The server names its own version in `initialize`; showing it is what makes
  // a mismatch between the extension and the binary visible rather than
  // mysterious.
  const info = client.initializeResult?.serverInfo;
  if (info?.version) {
    client.outputChannel.appendLine(`camello ${info.version}`);
  }
}

async function stop(): Promise<void> {
  const running = client;
  client = undefined;
  if (running) {
    await running.dispose();
  }
}

