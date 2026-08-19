import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(_context: vscode.ExtensionContext) {
    const cfg = vscode.workspace.getConfiguration('opensipsLsp');
    const serverOptions: ServerOptions = {
        command: cfg.get<string>('serverPath', 'opensips-lsp'),
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: 'opensips-cfg' }],
        initializationOptions: {
            opensipsPath: cfg.get<string>('opensipsPath', 'opensips'),
            opensipsSrc: cfg.get<string>('opensipsSrc', ''),
            checkTimeoutMs: cfg.get<number>('checkTimeoutMs', 10000),
        },
    };
    client = new LanguageClient(
        'opensipsLsp',
        'OpenSIPS LSP',
        serverOptions,
        clientOptions,
    );
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
