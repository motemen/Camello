# Camello for VS Code

A thin client for `camello lsp`. It finds the server, spawns `camello lsp`, and
does nothing else: everything the server answers — diagnostics as you type, the
inferred type and a sub's signature on hover, the methods a receiver's class
actually has after `->`, an outline, go-to-definition, whole-file formatting —
is the server's, and is described in [docs/lsp.md](https://github.com/motemen/Camello/blob/main/docs/lsp.md).

## Install

There is no marketplace release. Build the server, then package the extension:

```bash
cargo install --path ../..        # puts `camello` on PATH
cd editors/vscode
npm install
npx vsce package                  # camello-0.1.1.vsix
code --install-extension camello-0.1.1.vsix
```

During development, `npm run watch` and F5 from this folder opens an Extension
Development Host with the extension loaded.

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
