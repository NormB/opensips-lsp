// What the extension DOES when an unassociated `.cfg` is opened.
//
// The Rust gate (`an_included_fragment_is_associated_at_runtime`) can
// only see that the strings are present in the source; it cannot see
// whether the file is actually re-associated, whether a config another
// extension owns is left alone, or whether the switch works.  Those are
// the behaviours, and none of them is visible to `tsc` either.
//
// The compiled extension is loaded for real with `vscode` and
// `vscode-languageclient/node` stubbed, so this exercises the shipped
// code rather than a transcription of it.  Run with `npm test` after
// `npm run compile`.

const assert = require('assert');
const Module = require('module');
const path = require('path');

const LANGUAGE_ID = 'opensips-cfg';
const METHOD = 'opensips/analysisRoot';
const SETTING = 'associateIncludedFiles';

const calls = { setLanguage: [], requests: [] };
let onOpen = null;
let settingOn = true;
let openDocs = [];

/** A document as VS Code would hand it over. */
const doc = (fileName, languageId) => ({
    fileName,
    languageId,
    uri: { scheme: 'file', toString: () => 'file://' + fileName },
});

const vscodeStub = {
    workspace: {
        isTrusted: true,
        get textDocuments() {
            return openDocs;
        },
        getConfiguration: () => ({
            get: (key, dflt) => (key === SETTING ? settingOn : dflt),
        }),
        onDidOpenTextDocument: (cb) => {
            onOpen = cb;
            return { dispose() {} };
        },
        onDidGrantWorkspaceTrust: () => ({ dispose() {} }),
        onDidChangeConfiguration: () => ({ dispose() {} }),
    },
    languages: {
        setTextDocumentLanguage: async (d, id) => {
            calls.setLanguage.push([d.fileName, id]);
        },
    },
};

/** A server that includes everything under `inc/` and nothing else. */
class LanguageClient {
    async start() {}
    async stop() {}
    async sendRequest(method, params) {
        calls.requests.push([method, params.uri]);
        assert.strictEqual(method, METHOD, 'unexpected request method');
        return params.uri.includes('/inc/') ? 'file:///w/opensips.cfg' : null;
    }
    async sendNotification() {}
}

const load = Module._load;
Module._load = function (request) {
    if (request === 'vscode') {
        return vscodeStub;
    }
    if (request === 'vscode-languageclient/node') {
        return { LanguageClient, DidChangeConfigurationNotification: { type: 'x' } };
    }
    return load.apply(this, arguments);
};

const ext = require(path.join(__dirname, '..', 'out', 'extension.js'));

/** Let the association's promise chain run to completion. */
const settle = () => new Promise((r) => setTimeout(r, 30));

const fire = async (d) => {
    calls.setLanguage.length = 0;
    calls.requests.length = 0;
    await onOpen(d);
    await settle();
};

(async () => {
    // Documents already on screen when the server came up: didOpen has
    // been and gone for those, so only the startup sweep reaches them.
    openDocs = [doc('/w/inc/routes.cfg', 'plaintext')];
    ext.activate({ extensionPath: '/nonexistent', subscriptions: [] });
    await settle();
    assert.deepStrictEqual(
        calls.setLanguage,
        [['/w/inc/routes.cfg', LANGUAGE_ID]],
        'a fragment already open at startup must be associated',
    );

    await fire(doc('/w/inc/carriers.cfg', 'plaintext'));
    assert.deepStrictEqual(
        calls.setLanguage,
        [['/w/inc/carriers.cfg', LANGUAGE_ID]],
        'an included fragment must get the language',
    );

    await fire(doc('/w/wpa_supplicant.cfg', 'plaintext'));
    assert.deepStrictEqual(
        calls.setLanguage,
        [],
        'a .cfg nothing includes is not ours to claim',
    );

    // The whole point of not claiming `*.cfg` statically: a config
    // another extension owns must not be taken, and must not even be
    // asked about.
    await fire(doc('/w/inc/other.cfg', 'ini'));
    assert.deepStrictEqual(calls.setLanguage, [], 'claimed file hijacked');
    assert.deepStrictEqual(calls.requests, [], 'claimed file asked about');

    // A fragment is named whatever its author felt like, and a split
    // OpenSIPS tree usually names them `.inc`. The extension test
    // therefore cannot be the filename — the server is the one that
    // knows, so ask it and let the answer decide.
    await fire(doc('/w/inc/globals.inc', 'plaintext'));
    assert.deepStrictEqual(
        calls.setLanguage,
        [['/w/inc/globals.inc', LANGUAGE_ID]],
        'an included fragment not named .cfg must get the language',
    );

    await fire(doc('/w/notes.txt', 'plaintext'));
    assert.deepStrictEqual(
        calls.requests,
        [[METHOD, 'file:///w/notes.txt']],
        'a plaintext file must be asked about — the answer decides, not the suffix',
    );
    assert.deepStrictEqual(
        calls.setLanguage,
        [],
        'a plaintext file nothing includes is not ours to claim',
    );

    // Re-associating closes and reopens the document, so the handler
    // sees its own result: it must not ask again, or every fragment
    // would loop.
    await fire(doc('/w/inc/routes.cfg', LANGUAGE_ID));
    assert.deepStrictEqual(calls.requests, [], 're-association loop');

    settingOn = false;
    await fire(doc('/w/inc/routes.cfg', 'plaintext'));
    assert.deepStrictEqual(calls.setLanguage, [], 'the switch must turn it off');
    assert.deepStrictEqual(calls.requests, [], 'the switch must stop the request too');

    console.log('association: 11 assertions passed');
})().catch((e) => {
    console.error(e);
    process.exit(1);
});
