import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
/// What the server says it is judging `modparam` names against.
/// Shown only while an OpenSIPS config is in front of the user: a
/// permanent item would be noise in every other editor.
let catalogueStatus: vscode.StatusBarItem | undefined;
let catalogueText: string | undefined;

const OPENSIPS_LANGUAGE = 'opensips-cfg';

/// Show the item when the active editor is an OpenSIPS config and the
/// server has said what it is using; hide it otherwise.
function refreshCatalogueStatus(): void {
    if (!catalogueStatus) {
        return;
    }
    const active = vscode.window.activeTextEditor;
    const relevant = active?.document?.languageId === OPENSIPS_LANGUAGE;
    if (!relevant) {
        catalogueStatus.hide();
        return;
    }
    // a silenced editor looks like a broken one, so say so
    const on = vscode.workspace
        .getConfiguration('opensipsLsp')
        .get<boolean>('assistance', true);
    if (!on) {
        catalogueStatus.text = '$(circle-slash) OpenSIPS hints off';
        catalogueStatus.show();
        return;
    }
    if (catalogueText) {
        catalogueStatus.text = `$(book) ${catalogueText}`;
        catalogueStatus.show();
    } else {
        catalogueStatus.hide();
    }
}

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
            inlayHintParameterNames: cfg.get<boolean>('inlayHints.parameterNames', true),
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

/** Give an included fragment the language its root has.
 *
 *  VS Code cannot know that `carrier-routes.cfg` is part of an
 *  OpenSIPS configuration. The extension deliberately does not claim
 *  every `.cfg` — that would hijack unrelated config files — and a
 *  fragment is named whatever its author felt like, so no filename
 *  pattern reaches it. The configuration that includes it does know,
 *  and the server has read it: ask.
 *
 *  Asking is the whole point, so there is no suffix test here either:
 *  a split tree usually names its fragments `.inc`, and requiring
 *  `.cfg` was the same filename guess this exists to avoid. The
 *  server answers from an include graph it has already built, and a
 *  file nothing includes gets `null` and is left alone.
 *
 *  Only files VS Code left as plain text are touched. A file some
 *  other extension has already claimed belongs to that extension, and
 *  taking it would be exactly the hijack this avoids. */
async function associateIfIncluded(doc: vscode.TextDocument): Promise<void> {
    if (!client || doc.uri.scheme !== 'file') {
        return;
    }
    if (doc.languageId !== 'plaintext') {
        return;
    }
    if (!vscode.workspace
        .getConfiguration('opensipsLsp')
        .get<boolean>('associateIncludedFiles', true)) {
        return;
    }
    let root: string | null = null;
    try {
        root = await client.sendRequest<string | null>('opensips/analysisRoot', {
            uri: doc.uri.toString(),
        });
    } catch {
        // server still starting, or an older one without the request:
        // leaving the file as it is beats guessing
        return;
    }
    if (root) {
        await vscode.languages.setTextDocumentLanguage(doc, 'opensips-cfg');
    }
}

/** Documents already on screen when the server came up: didOpen has
 *  been and gone for those, so nothing else would ever revisit them. */
function associateOpenDocuments(): void {
    for (const doc of vscode.workspace.textDocuments) {
        void associateIfIncluded(doc);
    }
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
    if (client) {
        await client.stop();
    }
    client = buildClient(context);
    await client.start();
    associateOpenDocuments();
}

export function activate(context: vscode.ExtensionContext) {
    if (!vscode.workspace
        .getConfiguration('opensipsLsp')
        .get<boolean>('enable', true)) {
        return;
    }
    client = buildClient(context);
    catalogueStatus = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100,
    );
    catalogueStatus.tooltip =
        'The OpenSIPS release this file\u2019s modparam names are checked against';
    catalogueStatus.command = 'workbench.action.openSettings';
    context.subscriptions.push(catalogueStatus);
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => refreshCatalogueStatus()),
    );
    // one key turns the popups off and the same key turns them back
    // on; written to the workspace when there is one, so it does not
    // silently change every other project the reader opens
    context.subscriptions.push(
        vscode.commands.registerCommand('opensipsLsp.toggleAssistance', async () => {
            const cfg = vscode.workspace.getConfiguration('opensipsLsp');
            const target = vscode.workspace.workspaceFolders?.length
                ? vscode.ConfigurationTarget.Workspace
                : vscode.ConfigurationTarget.Global;
            await cfg.update('assistance', !cfg.get<boolean>('assistance', true), target);
        }),
    );
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('opensipsLsp.assistance')) {
                refreshCatalogueStatus();
            }
        }),
    );
    void client.start().then(() => {
        // the server names its catalogue once it is settled, and again
        // whenever it changes
        client?.onNotification(
            'opensipsLsp/catalogue',
            (p: { describe?: string }) => {
                catalogueText = p?.describe;
                refreshCatalogueStatus();
            },
        );
        return associateOpenDocuments();
    });
    context.subscriptions.push(
        vscode.workspace.onDidOpenTextDocument((d) => void associateIfIncluded(d)),
    );
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
        'opensipsLsp.opensipsVersion',
        'opensipsLsp.versionInHints',
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
                    inlayHintParameterNames: cfg.get<boolean>('inlayHints.parameterNames', true),
                    assistance: cfg.get<boolean>('assistance', true),
                },
            });
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
