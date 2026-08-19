import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

function buildClient(): LanguageClient {
    const cfg = vscode.workspace.getConfiguration('opensipsLsp');
    // Trust gate: 'opensips -C' dlopens the modules the cfg loads —
    // code execution. In untrusted workspaces diagnostics are forced
    // off regardless of settings; everything else keeps working.
    const opensipsPath = vscode.workspace.isTrusted
        ? cfg.get<string>('opensipsPath', 'opensips')
        : '';
    const serverOptions: ServerOptions = {
        command: cfg.get<string>('serverPath', 'opensips-lsp'),
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: 'opensips-cfg' }],
        initializationOptions: {
            opensipsPath,
            opensipsSrc: cfg.get<string>('opensipsSrc', ''),
            checkTimeoutMs: cfg.get<number>('checkTimeoutMs', 10000),
        },
    };
    return new LanguageClient(
        'opensipsLsp',
        'OpenSIPS LSP',
        serverOptions,
        clientOptions,
    );
}

async function restart(): Promise<void> {
    if (client) {
        await client.stop();
    }
    client = buildClient();
    await client.start();
}

export function activate(context: vscode.ExtensionContext) {
    client = buildClient();
    client.start();
    // trust granted later: restart so diagnostics come alive with the
    // configured opensipsPath
    context.subscriptions.push(
        vscode.workspace.onDidGrantWorkspaceTrust(() => void restart()),
    );
    // settings changed: restart with the new configuration
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('opensipsLsp')) {
                void restart();
            }
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
