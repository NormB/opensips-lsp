// Every configuration name the extension reaches for, checked against
// the manifest that publishes them.
//
// The other two suites stub `getConfiguration` as a function that
// ignores its argument, so the SECTION the extension asks for is
// invisible to them.  Renaming the namespace at all of its sites in the
// compiled extension leaves both of them green while a user's settings
// go unread and every default silently applies — which is how this file
// came to exist.  The manifest is the only place these names are
// published, so it is the side compared against, and the prefix is read
// out of it rather than written here again: one copy of the fact, not a
// second one to drift.

const assert = require('assert');
const Module = require('module');
const path = require('path');
const fs = require('fs');

const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8'),
);
const declared = Object.keys(manifest.contributes.configuration.properties);
const prefixes = [...new Set(declared.map((k) => k.split('.')[0]))];
assert.strictEqual(
    prefixes.length,
    1,
    `the manifest declares settings under more than one prefix: ${prefixes}`,
);
const NAMESPACE = prefixes[0];
const declaredCommands = (manifest.contributes.commands ?? []).map(
    (c) => c.command,
);

/** What the extension ASKED for, which is what the other suites drop. */
let sections = [];
let readKeys = [];
let writtenKeys = [];
let affected = [];

let registeredCommands = {};
let configHandlers = [];
let statusItem = null;
let openHandler = null;
let trusted = true;
let settings = {};

const vscodeStub = {
    workspace: {
        get isTrusted() {
            return trusted;
        },
        textDocuments: [],
        workspaceFolders: undefined,
        getConfiguration: (section) => {
            sections.push(section);
            return {
                get: (key, dflt) => {
                    readKeys.push(key);
                    return key in settings ? settings[key] : dflt;
                },
                update: async (key, value) => {
                    writtenKeys.push(key);
                    settings[key] = value;
                },
            };
        },
        onDidOpenTextDocument: (cb) => {
            openHandler = cb;
            return { dispose() {} };
        },
        onDidGrantWorkspaceTrust: () => ({ dispose() {} }),
        onDidChangeConfiguration: (cb) => {
            configHandlers.push(cb);
            return { dispose() {} };
        },
    },
    languages: { setTextDocumentLanguage: async () => {} },
    window: {
        activeTextEditor: undefined,
        onDidChangeActiveTextEditor: () => ({ dispose() {} }),
        createStatusBarItem: () => {
            statusItem = {
                text: '',
                tooltip: '',
                command: '',
                show() {},
                hide() {},
                dispose() {},
            };
            return statusItem;
        },
    },
    commands: {
        registerCommand: (id, cb) => {
            registeredCommands[id] = cb;
            return { dispose() {} };
        },
    },
    ConfigurationTarget: { Global: 1, Workspace: 2 },
    StatusBarAlignment: { Left: 1, Right: 2 },
};

class LanguageClient {
    constructor(_id, _name, serverOptions, clientOptions) {
        this.serverOptions = serverOptions;
        this.clientOptions = clientOptions;
    }
    onNotification() {
        return { dispose() {} };
    }
    async start() {}
    async stop() {}
    async sendRequest() {
        return null;
    }
    async sendNotification() {}
}

const load = Module._load;
Module._load = function (request) {
    if (request === 'vscode') return vscodeStub;
    if (request === 'vscode-languageclient/node') {
        return { LanguageClient, DidChangeConfigurationNotification: { type: 'cfg' } };
    }
    return load.apply(this, arguments);
};

const ext = require(path.join(__dirname, '..', 'out', 'extension.js'));

const settle = () => new Promise((r) => setTimeout(r, 30));
const ctx = () => ({ extensionPath: '/nonexistent-extension-dir', subscriptions: [] });

(async () => {
    ext.activate(ctx());
    await settle();

    // reach the command handler and both configuration listeners.  The
    // answer is given twice: `false` lets `Array.prototype.some` walk
    // the whole restart-setting list instead of stopping at the first
    // match, so every query the extension makes is seen.
    // a plain-text file on disk: the path that asks whether included
    // fragments may be re-associated, and the only reader of that setting
    await openHandler({
        fileName: '/tmp/fragment.inc',
        languageId: 'plaintext',
        uri: { scheme: 'file', toString: () => 'file:///tmp/fragment.inc' },
    });
    for (const cb of Object.values(registeredCommands)) await cb();
    for (const answer of [false, true]) {
        for (const cb of configHandlers) {
            await cb({
                affectsConfiguration: (q) => {
                    affected.push(q);
                    return answer;
                },
            });
        }
    }
    await settle();

    // POSITIVE CONTROLS.  An extension that asked for nothing would
    // satisfy every assertion below by vacuity, and so would a stub
    // wired to the wrong module.
    assert.ok(sections.length > 0, 'no getConfiguration call was observed');
    assert.ok(readKeys.length > 0, 'no setting was read');
    assert.ok(writtenKeys.length > 0, 'no setting was written');
    assert.ok(affected.length > 0, 'no affectsConfiguration query was observed');
    assert.ok(
        Object.keys(registeredCommands).length > 0,
        'no command was registered',
    );
    assert.ok(configHandlers.length > 0, 'no configuration listener was registered');

    for (const section of new Set(sections)) {
        assert.strictEqual(
            section,
            NAMESPACE,
            `the extension read configuration section '${section}', but the manifest publishes its settings under '${NAMESPACE}' — a user's settings would go unread`,
        );
    }

    for (const key of new Set([...readKeys, ...writtenKeys])) {
        assert.ok(
            declared.includes(`${NAMESPACE}.${key}`),
            `the extension uses '${NAMESPACE}.${key}', which the manifest does not declare — it can never be set in the settings UI`,
        );
    }

    for (const query of new Set(affected)) {
        assert.ok(
            query === NAMESPACE || declared.includes(query),
            `affectsConfiguration('${query}') names neither the namespace nor a declared setting — the change it watches for can never arrive`,
        );
    }

    for (const id of Object.keys(registeredCommands)) {
        assert.ok(
            declaredCommands.includes(id),
            `the extension registers command '${id}', which the manifest does not contribute`,
        );
    }

    // The reverse direction.  A setting the manifest publishes and
    // nothing consumes is one a user can set, and watch do nothing.
    // Every exclusion is named with the reason it is someone else's to
    // read, so the list cannot quietly grow to cover a defect.
    const READ_ELSEWHERE = {
        'trace.server':
            'vscode-languageclient reads it out of the client id section itself',
    };
    const used = new Set([...readKeys, ...writtenKeys]);
    const dead = declared
        .map((k) => k.slice(NAMESPACE.length + 1))
        .filter((k) => !used.has(k) && !(k in READ_ELSEWHERE));
    assert.deepStrictEqual(
        dead,
        [],
        `the manifest publishes ${dead.length} setting(s) the extension never reads: ${dead
            .map((k) => `${NAMESPACE}.${k}`)
            .join(', ')} — a user can set them and nothing happens`,
    );

    console.log(
        `settings: ${sections.length + readKeys.length + affected.length + declared.length} assertions passed`,
    );
})();
