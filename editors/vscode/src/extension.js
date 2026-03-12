const { workspace, window } = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function activate(context) {
  const config = workspace.getConfiguration('fard');
  const lspPath = config.get('lspPath', 'fard-lsp');

  const serverOptions = {
    run:   { command: lspPath, transport: TransportKind.stdio },
    debug: { command: lspPath, transport: TransportKind.stdio },
  };

  const clientOptions = {
    documentSelector: [{ scheme: 'file', language: 'fard' }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.fard'),
    },
  };

  client = new LanguageClient('fard-lsp', 'FARD Language Server', serverOptions, clientOptions);
  client.start();
  window.showInformationMessage('FARD language server started');
}

function deactivate() {
  if (client) return client.stop();
}

module.exports = { activate, deactivate };
