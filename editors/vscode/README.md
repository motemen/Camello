# Camello for VS Code

A thin client for `camello lsp`. It finds the server, spawns `camello lsp`, and
stays out of the way: everything the server answers — diagnostics as you type,
the inferred type and a sub's signature on hover, the methods a receiver's class
actually has after `->`, an outline, go-to-definition, whole-file formatting —
is the server's, and is described in [docs/lsp.md](https://github.com/motemen/Camello/blob/main/docs/lsp.md).

## Install

There is no marketplace release. The `.vsix` is attached to every release, so
installing is one download and one command:

```bash
gh release download --repo motemen/Camello --pattern '*.vsix'
code --install-extension camello-*.vsix
```

**No Rust toolchain is needed.** With no `camello` on your `PATH`, the extension
fetches the server for its own version from the same release the first time it
starts, checks it against the published `.sha256`, and keeps it. A `camello` you
already have — or one named by the `camello.path` setting — is used as is and
nothing is downloaded.

Prebuilt servers exist for macOS and Linux on both architectures. On anything
else, build one and point `camello.path` at it:

```bash
cargo install --path ../..        # puts `camello` on PATH
```

To build the extension yourself instead of downloading it:

```bash
cd editors/vscode
npm install
npx @vscode/vsce package
code --install-extension camello-*.vsix
```

During development, `npm run watch` and F5 from this folder opens an Extension
Development Host with the extension loaded.

## Checking the whole workspace

The server reports on the files you have open, and only those — nobody wants a
diagnostic about a file they are not looking at. For the whole tree there is
`camello check`, and the extension wires it to the Problems panel:

- **Camello: Check Workspace** from the command palette, or
- a task of type `camello`, which is what the command runs:

```json
{
  "type": "camello",
  "args": ["--min-severity", "warning"],
  "problemMatcher": "$camello"
}
```

The `$camello` matcher is contributed too, so any task of your own that runs
`camello check` can use it.

## Settings

| Setting | What it does |
| --- | --- |
| `camello.path` | Path to the `camello` binary. Empty means whatever is on `PATH`. |
| `camello.trace.server` | Log the traffic between the editor and the server. |

Everything else is `camello.toml` at the root of the workspace, which the
server reads itself — the same `[check]` table `camello check` reads. There is
no VS Code dialect of it, so an eglot or nvim-lspconfig user pointing at
`camello lsp` gets the identical server:

```lua
-- nvim-lspconfig
require("lspconfig.configs").camello = {
  default_config = {
    cmd = { "camello", "lsp" },
    filetypes = { "perl" },
    root_dir = require("lspconfig.util").root_pattern("camello.toml", ".git"),
  },
}
```

```elisp
;; eglot
(add-to-list 'eglot-server-programs '(perl-mode . ("camello" "lsp")))
```

## Formatting

The server formats a whole file, and refuses to format one it cannot fully
parse — the same rule `camello format` follows. On-save formatting is the
editor's choice:

```json
"[perl]": { "editor.formatOnSave": true, "editor.defaultFormatter": "camello.camello" }
```
