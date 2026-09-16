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

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
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

  const options: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "perl" }],
    synchronize: {
      // The server watches these itself; this is what makes VS Code send the
      // events for the dynamic registration it asks for at startup.
      fileEvents: vscode.workspace.createFileSystemWatcher(
        "**/{*.pl,*.pm,*.t,*.psgi,camello.toml}",
      ),
    },
    outputChannel: vscode.window.createOutputChannel("Camello"),
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
  context.subscriptions.push(client);

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
    await running.stop();
  }
}

