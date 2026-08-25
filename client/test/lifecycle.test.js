// The extension paths that are NOT the association.
//
// `association.test.js` covers what happens when an unassociated
// `.cfg` is opened.  Everything else the extension does — refusing to
// start when disabled, forcing the checker off in an untrusted
// folder, restarting when trust arrives, telling a restart-shaped
// setting from a live one, shutting down — had no test at all, and
// one of those is a security property with only a manifest check
// behind it.
//
// The compiled extension is loaded for real with `vscode` and
// `vscode-languageclient/node` stubbed, so this exercises the shipped
// code rather than a transcription of it.

const assert = require('assert');
const Module = require('module');
const path = require('path');

const NAMESPACE = 'opensipsLsp';
const BINARY_SETTING = 'opensipsPath';

/** Everything the extension did, in order. */
let log = [];
let settings = {};
let trusted = true;
let handlers = {};
let clients = [];

const doc = (fileName, languageId) => ({
    fileName,
    languageId,
    uri: { scheme: 'file', toString: () => 'file://' + fileName },
});

const vscodeStub = {
    workspace: {
        get isTrusted() {
            return trusted;
        },
        textDocuments: [],
        getConfiguration: () => ({
            get: (key, dflt) => (key in settings ? settings[key] : dflt),
        }),
        onDidOpenTextDocument: (cb) => {
            handlers.open = cb;
            return { dispose() {} };
        },
        onDidGrantWorkspaceTrust: (cb) => {
            handlers.trust = cb;
            return { dispose() {} };
        },
        onDidChangeConfiguration: (cb) => {
            handlers.config = cb;
            return { dispose() {} };
        },
    },
    languages: { setTextDocumentLanguage: async () => {} },
};

class LanguageClient {
    constructor(_id, _name, serverOptions, clientOptions) {
        this.serverOptions = serverOptions;
        this.clientOptions = clientOptions;
        this.notifications = [];
        clients.push(this);
        log.push('build');
    }
    async start() {
        log.push('start');
    }
    async stop() {
        log.push('stop');
    }
    async sendRequest() {
        return null;
    }
    async sendNotification(_type, params) {
        this.notifications.push(params);
        log.push('notify');
    }
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
const reset = () => {
    log = [];
    settings = {};
    trusted = true;
    handlers = {};
    clients = [];
};
/** A context whose extensionPath holds no bundled server. */
const ctx = () => ({ extensionPath: '/nonexistent-extension-dir', subscriptions: [] });

/** The `initializationOptions` the most recent client was built with. */
const lastInit = () => clients[clients.length - 1].clientOptions.initializationOptions;

(async () => {
    // --- disabled: nothing is started at all
    reset();
    settings.enable = false;
    ext.activate(ctx());
    await settle();
    assert.deepStrictEqual(log, [], 'enable=false must not start a server');
    assert.strictEqual(clients.length, 0);

    // --- the trust gate, which is why this file exists.
    // `opensips -C` dlopens the modules a config loads, so checking a
    // config in a folder you have not trusted is arbitrary code
    // execution from a repository you just cloned.  The manifest
    // restricts the SETTING in untrusted workspaces; this is the
    // runtime half.
    reset();
    trusted = false;
    settings[BINARY_SETTING] = '/usr/sbin/evil';
    ext.activate(ctx());
    await settle();
    assert.strictEqual(
        lastInit()[BINARY_SETTING],
        '',
        'an untrusted folder must not hand the server a checker to run',
    );

    // trusted, and the checker is passed through
    reset();
    trusted = true;
    settings[BINARY_SETTING] = '/usr/sbin/opensips';
    ext.activate(ctx());
    await settle();
    assert.strictEqual(
        lastInit()[BINARY_SETTING],
        '/usr/sbin/opensips',
        'a trusted folder gets the configured checker',
    );

    // trusted, but diagnostics switched off: same result, different reason
    reset();
    trusted = true;
    settings[BINARY_SETTING] = '/usr/sbin/opensips';
    settings['diagnostics.enable'] = false;
    ext.activate(ctx());
    await settle();
    assert.strictEqual(
        lastInit()[BINARY_SETTING],
        '',
        'diagnostics off means no checker path either',
    );

    // --- trust granted later: the server is rebuilt, so the checker
    // it was denied at startup arrives without a window reload
    reset();
    trusted = false;
    settings[BINARY_SETTING] = '/usr/sbin/opensips';
    ext.activate(ctx());
    await settle();
    assert.strictEqual(lastInit()[BINARY_SETTING], '', 'denied while untrusted');
    trusted = true;
    await handlers.trust();
    await settle();
    assert.ok(log.includes('stop'), 'the old server is stopped');
    assert.strictEqual(clients.length, 2, 'and a new one built');
    assert.strictEqual(
        lastInit()[BINARY_SETTING],
        '/usr/sbin/opensips',
        'the checker arrives with trust, without a reload',
    );

    // --- a restart-shaped setting restarts; a runtime one is pushed
    reset();
    ext.activate(ctx());
    await settle();
    const before = clients.length;
    await handlers.config({ affectsConfiguration: (s) => s === NAMESPACE || s.endsWith('.serverPath') });
    await settle();
    assert.ok(clients.length > before, 'serverPath must rebuild the client');

    reset();
    ext.activate(ctx());
    await settle();
    const built = clients.length;
    await handlers.config({
        affectsConfiguration: (s) => s === NAMESPACE || s.endsWith('diagnostics.analyzer'),
    });
    await settle();
    assert.strictEqual(clients.length, built, 'a runtime toggle must NOT rebuild');
    const pushed = clients[clients.length - 1].notifications;
    assert.strictEqual(pushed.length, 1, 'it is pushed to the running server');
    // The two extensions send this differently — one wraps the block
    // in its namespace, the other sends it flat — and both servers
    // accept either (`settings.get(NAMESPACE).unwrap_or(settings)`).
    // Asserting one shape would be transcribing this client rather
    // than testing the contract; what matters is that the toggle
    // actually travels.
    const block = pushed[0].settings[NAMESPACE] ?? pushed[0].settings;
    assert.ok(
        'analyzerDiagnostics' in block,
        `the push must carry the runtime toggles: ${JSON.stringify(pushed[0])}`,
    );

    // a change in someone else's settings is not ours to react to
    reset();
    ext.activate(ctx());
    await settle();
    const untouched = clients.length;
    await handlers.config({ affectsConfiguration: () => false });
    await settle();
    assert.strictEqual(clients.length, untouched, 'an unrelated setting is ignored');
    assert.strictEqual(clients[0].notifications.length, 0, 'and nothing is pushed');

    // --- deactivate stops the server rather than leaking it
    reset();
    ext.activate(ctx());
    await settle();
    await ext.deactivate();
    assert.ok(log.includes('stop'), 'deactivate must stop the client');

    console.log('lifecycle: 15 assertions passed');
})().catch((e) => {
    console.error(e);
    process.exit(1);
});
