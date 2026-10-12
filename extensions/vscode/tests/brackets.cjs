
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { buildSync } = require('esbuild');


// This fixture reports editor state. Assertions and test orchestration stay in Shelly.
const input = JSON.parse(readFileSync(0, 'utf8'));
const listeners = new Map();
const decorations = [];
const editors = [];
const document = {
    uri: { toString: () => 'file:///brackets.shy' },
    languageId: 'shelly', version: 1, isClosed: false, source: input.source,
    get lineCount() { return this.source.split('\n').length; },
    lineAt(line) { return { text: this.source.split('\n')[line] }; }
};
let enabled = true;
let semantic = true;
function subscribe(event, listener)
{
    listeners.set(event, listener);
    return { dispose() { listeners.delete(event); } };
}
function editor()
{
    return { document, colors: [[], [], [], []],
        setDecorations(decoration, ranges) { this.colors[decoration.index] = ranges; } };
}
editors.push(editor());
const vscode = {
    ThemeColor: class { constructor(id) { this.id = id; } },
    Range: class { constructor(...positions) { this.positions = positions; } },
    DecorationRangeBehavior: { ClosedClosed: 1 },
    workspace: {
        getConfiguration(section)
        { return { get() { return section === 'editor' ? semantic : enabled; } }; },
        onDidChangeTextDocument: listener => subscribe('edit', listener),
        onDidCloseTextDocument: listener => subscribe('close', listener),
        onDidChangeConfiguration: listener => subscribe('configuration', listener)
    },
    window: {
        visibleTextEditors: editors,
        onDidChangeVisibleTextEditors: listener => subscribe('visible', listener),
        createTextEditorDecorationType(options)
        {
            const decoration = { index: decorations.length, options, disposed: false,
                dispose()
                {
                    this.disposed = true;
                    for (const editor of editors) { editor.colors[this.index] = []; }
                } };
            decorations.push(decoration);
            return decoration;
        }
    }
};
const compiled = buildSync({ entryPoints: [resolve('src/brackets.ts')], bundle: true,
    platform: 'node', format: 'cjs', write: false, external: ['vscode'] });
const moduleObject = { exports: {} };
new Function('require', 'module', 'exports', compiled.outputFiles[0].text)(
    name => { if (name === 'vscode') { return vscode; } throw new Error(name); },
    moduleObject, moduleObject.exports);
const painter = new moduleObject.exports.BracketColors();
const frames = [];
for (const action of input.actions)
{
    switch (action.op)
    {
        case 'accept':
            painter.accept(document, action.version ?? document.version,
                { data: new Uint32Array(action.data ?? input.data),
                    resultId: action.resultId ?? String(document.version) }, input.legend);
            break;
        case 'edit':
            document.source = action.source ?? document.source;
            document.version++;
            listeners.get('edit')?.({ document, contentChanges: [{}] });
            break;
        case 'split': editors.push(editor()); listeners.get('visible')?.(editors); break;
        case 'configure':
            enabled = action.enabled ?? enabled;
            semantic = action.semantic ?? semantic;
            listeners.get('configuration')?.({ affectsConfiguration: () => true });
            break;
        case 'language':
            document.languageId = action.language;
            listeners.get('visible')?.(editors);
            break;
        case 'close': document.isClosed = true; listeners.get('close')?.(document); break;
        case 'dispose': painter.dispose(); break;
        default: throw new Error(`Unknown fixture action: ${action.op}`);
    }
    frames.push(editors.map(editor => editor.colors.map(ranges =>
        ranges.map(range => range.positions))));
}
console.log(JSON.stringify({ frames, palette: decorations.map(item => item.options.color.id),
    disposed: decorations.every(item => item.disposed), subscriptions: listeners.size }));
