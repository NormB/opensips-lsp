import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/** Resolve the server binary: an explicit non-default setting wins,
 *  then the binary bundled inside platform-specific builds of this
 *  extension, then a PATH lookup. */
function serverCommand(context: vscode.ExtensionContext): string {
    const cfg = vscode.workspace.getConfiguration('opensipsLsp');
    const configured = cfg.get<string>('serverPath', 'opensips-lsp');
    if (configured && configured !== 'opensips-lsp') {
        return configured;
    }
    const exe = process.platform === 'win32' ? 'opensips-lsp.exe' : 'opensips-lsp';
    const bundled = path.join(context.extensionPath, 'server', exe);
    if (fs.existsSync(bundled)) {
        try {
            fs.chmodSync(bundled, 0o755);
        } catch {
            // read-only extension dir: the mode from the package applies
        }
        return bundled;
    }
    return configured;
}

function buildClient(context: vscode.ExtensionContext): LanguageClient {
    const cfg = vscode.workspace.getConfiguration('opensipsLsp');
    const diagnosticsWanted = cfg.get<boolean>('diagnostics.enable', true);
    // Trust gate: 'opensips -C' dlopens the modules the cfg loads —
    // code execution. In untrusted workspaces diagnostics are forced
    // off regardless of settings; everything else keeps working.
    const opensipsPath =
        vscode.workspace.isTrusted && diagnosticsWanted
            ? cfg.get<string>('opensipsPath', 'opensips')
            : '';
    const serverOptions: ServerOptions = {
        command: serverCommand(context),
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: 'opensips-cfg' }],
        initializationOptions: {
            opensipsPath,
            opensipsSrc: cfg.get<string>('opensipsSrc', ''),
            checkTimeoutMs: cfg.get<number>('checkTimeoutMs', 10000),
            snippetCompletions: cfg.get<boolean>('completion.snippets', true),
            maxDiagnostics: cfg.get<number>('diagnostics.maxProblems', 100),
            analyzerDiagnostics: cfg.get<boolean>('diagnostics.analyzer', true),
            codeLensReferences: cfg.get<boolean>('codeLens.references', true),
            cacheDir: cfg.get<string>('cacheDir', ''),
        },
    };
    return new LanguageClient(
        'opensipsLsp',
        'OpenSIPS LSP',
        serverOptions,
        clientOptions,
    );
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
    if (client) {
        await client.stop();
    }
    client = buildClient(context);
    await client.start();
}

export function activate(context: vscode.ExtensionContext) {
    if (!vscode.workspace
        .getConfiguration('opensipsLsp')
        .get<boolean>('enable', true)) {
        return;
    }
    client = buildClient(context);
    client.start();
    // trust granted later: restart so diagnostics come alive with the
    // configured opensipsPath
    context.subscriptions.push(
        vscode.workspace.onDidGrantWorkspaceTrust(() => void restart(context)),
    );
    // settings changed: paths and trust-gated settings need a fresh
    // server; runtime toggles are pushed live over
    // workspace/didChangeConfiguration
    const restartSettings = [
        'opensipsLsp.serverPath',
        'opensipsLsp.opensipsPath',
        'opensipsLsp.opensipsSrc',
        'opensipsLsp.cacheDir',
        'opensipsLsp.enable',
        'opensipsLsp.diagnostics.enable',
        'opensipsLsp.trace.server',
    ];
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (!e.affectsConfiguration('opensipsLsp')) {
                return;
            }
            if (restartSettings.some((k) => e.affectsConfiguration(k))) {
                void restart(context);
                return;
            }
            const cfg = vscode.workspace.getConfiguration('opensipsLsp');
            void client?.sendNotification('workspace/didChangeConfiguration', {
                settings: {
                    checkTimeoutMs: cfg.get<number>('checkTimeoutMs', 10000),
                    snippetCompletions: cfg.get<boolean>('completion.snippets', true),
                    maxDiagnostics: cfg.get<number>('diagnostics.maxProblems', 100),
                    analyzerDiagnostics: cfg.get<boolean>('diagnostics.analyzer', true),
                    codeLensReferences: cfg.get<boolean>('codeLens.references', true),
                },
            });
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
