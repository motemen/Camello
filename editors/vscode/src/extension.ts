// The thin half of `camello lsp` (docs/lsp.md, "The VS Code extension").
//
// Find the server, spawn `camello lsp`, hand the connection to
// vscode-languageclient, and get out of the way. Everything a user can
// configure here is a pass-through of the server's own configuration —
// camello.toml is read by the server, not by this file — so an eglot or
// nvim-lspconfig user who points at `camello lsp` themselves gets the
// identical server.

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("camello.restartServer", async () => {
      await stop();
      await start(context);
    }),
  );
  await start(context);
}

export async function deactivate(): Promise<void> {
  await stop();
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  const command = serverPath();
  const server: ServerOptions = {
    run: { command, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command, args: ["lsp"], transport: TransportKind.stdio },
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

function serverPath(): string {
  const configured = vscode.workspace
    .getConfiguration("camello")
    .get<string>("path");
  return configured && configured.length > 0 ? configured : "camello";
}
