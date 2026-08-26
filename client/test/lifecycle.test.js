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
let statusItem = null;
let activeEditor = null;
let registeredCommands = {};
let updates = [];

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
            update: async (key, value, target) => {
                updates.push([key, value, target]);
                settings[key] = value;
            },
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
    languages: { setTextDocumentLanguage: async () => {} },    window: {
        get activeTextEditor() {
            return activeEditor;
        },
        onDidChangeActiveTextEditor: (cb) => {
            handlers.activeEditor = cb;
            return { dispose() {} };
        },
        createStatusBarItem: () => {
            statusItem = {
                text: '',
                tooltip: '',
                command: '',
                shown: false,
                show() {
                    this.shown = true;
                },
                hide() {
                    this.shown = false;
                },
                dispose() {},
            };
            return statusItem;
        },
    },
    // The extension registers the assistance toggle as a command; a
    // stub without `commands` makes `activate` throw before anything
    // else this file asserts can run.
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
        this.notifications = [];
        clients.push(this);
        log.push('build');
    }
    onNotification(method, cb) {
        this.handlers = this.handlers ?? {};
        this.handlers[method] = cb;
        return { dispose() {} };
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
    statusItem = null;
    activeEditor = null;
    registeredCommands = {};
    updates = [];
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

    // --- the release the server uses is shown, and only where it applies
    //
    // A warning names the catalogue, but only once something is
    // wrong. Until then nothing told the reader which OpenSIPS
    // release their file was being parsed against.
    reset();
    ext.activate(ctx());
    await settle();
    const announce = clients[clients.length - 1].handlers['opensipsLsp/catalogue'];
    assert.ok(announce, 'the client must subscribe to the catalogue announcement');

    activeEditor = { document: { languageId: 'opensips-cfg' } };
    handlers.activeEditor?.();
    assert.strictEqual(
        statusItem.shown,
        false,
        'nothing is shown until the server names its catalogue',
    );

    announce({ describe: 'OpenSIPS 4.0.1 (built in)', version: '4.0.1' });
    assert.strictEqual(statusItem.shown, true, 'it appears once announced');
    assert.ok(
        statusItem.text.includes('OpenSIPS 4.0.1'),
        `the release must be visible: ${statusItem.text}`,
    );

    activeEditor = { document: { languageId: 'plaintext' } };
    handlers.activeEditor?.();
    assert.strictEqual(
        statusItem.shown,
        false,
        'a permanent item would be noise in every other editor',
    );

    activeEditor = { document: { languageId: 'opensips-cfg' } };
    handlers.activeEditor?.();
    announce({ describe: 'the configured source tree' });
    assert.ok(
        statusItem.text.includes('configured source tree'),
        `a configured tree must say so: ${statusItem.text}`,
    );

    // --- changing the release restarts, because the server reads it once
    //
    // It is taken from `initializationOptions`, so pushing it to a
    // running server would change nothing and the user would see the
    // old release keep answering.
    reset();
    ext.activate(ctx());
    await settle();
    const beforeVersion = clients.length;
    await handlers.config({
        affectsConfiguration: (s) => s === NAMESPACE || s.endsWith('.opensipsVersion'),
    });
    await settle();
    assert.ok(
        clients.length > beforeVersion,
        'changing the release must rebuild the client, not be pushed',
    );

    // --- the assistance toggle flips the setting, both ways
    //
    // Owed for the CI failure this change caused: the extension began
    // registering a command and the stub had no `commands`, so
    // `activate` threw before any assertion in this file ran. A stub
    // extended just enough to stop throwing would have left the
    // command itself untested — which is the part that broke.
    reset();
    ext.activate(ctx());
    await settle();
    const toggle = registeredCommands[`${NAMESPACE}.toggleAssistance`];
    assert.ok(toggle, 'the toggle command must be registered');

    settings['assistance'] = true;
    await toggle();
    assert.deepStrictEqual(
        updates.at(-1).slice(0, 2),
        ['assistance', false],
        `pressing it while on must turn it off: ${JSON.stringify(updates)}`,
    );
    await toggle();
    assert.deepStrictEqual(
        updates.at(-1).slice(0, 2),
        ['assistance', true],
        'and pressing it again must turn it back on',
    );

    // --- and the status bar says so, since a silent editor looks broken
    //
    // Owed, second. The reader has no other signal that the popups
    // were turned off on purpose.
    reset();
    settings['assistance'] = false;
    ext.activate(ctx());
    await settle();
    activeEditor = { document: doc('/w/opensips.cfg', 'opensips-cfg') };
    handlers.activeEditor?.();
    assert.ok(
        /off/i.test(statusItem.text),
        `the status bar must say the hints are off: ${statusItem.text}`,
    );
    assert.strictEqual(statusItem.shown, true, 'and it must be visible');

    console.log('lifecycle: 26 assertions passed');
})().catch((e) => {
    console.error(e);
    process.exit(1);
});
