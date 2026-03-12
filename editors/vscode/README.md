# FARD Language Support for VS Code

Syntax highlighting, diagnostics, and hover docs for .fard files.

## Features

- Syntax highlighting: keywords, strings, interpolation, numbers, operators, artifact and import forms
- Diagnostics: parse and eval errors shown inline as you type, powered by fard-lsp
- Hover: documentation for keywords and all 25 stdlib modules on hover

## Setup

Build fard-lsp and install it:

    cargo build -p fard-lsp --release
    cp target/release/fard-lsp /usr/local/bin/

Install the extension:

    cd editors/vscode
    npm install
    vsce package
    code --install-extension fard-language-0.1.0.vsix

Open any .fard file and the language server starts automatically.

## Configuration

fard.lspPath (default: "fard-lsp") — path to the fard-lsp binary.

## Hover Docs

Hovering over any keyword or stdlib alias shows inline documentation.

Keywords: let, fn, if, match, while, return, artifact, import
Stdlib: list, str, math, io, hash, json, re, http, witness
