// `camello check` over the whole workspace, as a task.
//
// The server publishes diagnostics for open files only, and that is deliberate
// (docs/lsp.md, "Diagnostics"): nobody is told about a broken caller in a file
// nobody is looking at. Answering for the whole tree is `camello check`'s job,
// so this runs exactly that and lets the `$camello` matcher put the output in
// the Problems panel — no second implementation of the diagnostics, and the
// same answer a CI run gives.

import * as vscode from "vscode";

const TYPE = "camello";

interface CheckTask extends vscode.TaskDefinition {
  type: typeof TYPE;
  /// Extra arguments for `camello check`, such as `--min-severity warning`.
  args?: string[];
}

export function register(
  context: vscode.ExtensionContext,
  server: () => string,
): void {
  context.subscriptions.push(
    vscode.tasks.registerTaskProvider(TYPE, {
      provideTasks: () => folders().map((folder) => task(server(), folder, [])),
      resolveTask: (candidate) => {
        const folder =
          candidate.scope instanceof Object && "uri" in candidate.scope
            ? (candidate.scope as vscode.WorkspaceFolder)
            : folders()[0];
        if (!folder) {
          return undefined;
        }
        const definition = candidate.definition as CheckTask;
        return task(server(), folder, definition.args ?? []);
      },
    }),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("camello.checkWorkspace", async () => {
      const chosen = await folder();
      if (!chosen) {
        vscode.window.showErrorMessage(
          "camello: open a folder to check — there is no workspace here.",
        );
        return;
      }
      await vscode.tasks.executeTask(task(server(), chosen, []));
    }),
  );
}

function folders(): readonly vscode.WorkspaceFolder[] {
  return vscode.workspace.workspaceFolders ?? [];
}

/// Which folder to check, asking only when the answer is not already one.
async function folder(): Promise<vscode.WorkspaceFolder | undefined> {
  const open = folders();
  if (open.length <= 1) {
    return open[0];
  }
  return await vscode.window.showWorkspaceFolderPick({
    placeHolder: "Which folder should camello check?",
  });
}

function task(
  server: string,
  folder: vscode.WorkspaceFolder,
  args: string[],
): vscode.Task {
  const definition: CheckTask = { type: TYPE, args };
  // `.` rather than the folder's path: the task runs with the folder as its
  // working directory, and a relative path is what keeps the diagnostics'
  // locations relative to it for the matcher to resolve.
  const command = new vscode.ShellExecution(
    server,
    ["check", ".", ...args],
    { cwd: folder.uri.fsPath },
  );
  const created = new vscode.Task(
    definition,
    folder,
    "check workspace",
    TYPE,
    command,
    "$camello",
  );
  // Nothing here is built, and the panel does not need to steal focus to say
  // so: the findings go to the Problems panel, which is where they are read.
  created.group = vscode.TaskGroup.Test;
  created.presentationOptions = {
    reveal: vscode.TaskRevealKind.Silent,
    panel: vscode.TaskPanelKind.Dedicated,
    clear: true,
  };
  return created;
}
