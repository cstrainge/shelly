
import * as vscode from 'vscode';


interface BracketRanges
{
    version: number;
    colors: vscode.Range[][];
}


/**
 * Only server-classified delimiters participate. This layer assigns nesting colors;
 * it never tokenizes source or attempts to recognize strings, comments, or expressions.
 */
export class BracketColors implements vscode.Disposable
{
    private readonly decorations: vscode.TextEditorDecorationType[];
    private readonly subscriptions: vscode.Disposable[];
    private readonly documents = new Map<string, BracketRanges>();
    private disposed = false;

    constructor()
    {
        // The first three theme colors have visible defaults; entries four to six
        // are transparent unless the theme overrides them.
        const colors = Array.from({ length: 3 }, (_, index) =>
            `editorBracketHighlight.foreground${index + 1}`);
        colors.push('editorBracketHighlight.unexpectedBracket.foreground');
        this.decorations = colors.map(color => vscode.window.createTextEditorDecorationType({
            color: new vscode.ThemeColor(color),
            rangeBehavior: vscode.DecorationRangeBehavior.ClosedClosed
        }));
        this.subscriptions = [
            vscode.workspace.onDidChangeTextDocument(event =>
            {
                // Clear immediately, including split editors, before the next response arrives.
                if (!event.contentChanges.length) { return; }
                this.forget(event.document);
            }),
            vscode.workspace.onDidCloseTextDocument(document => this.forget(document)),
            vscode.window.onDidChangeVisibleTextEditors(editors =>
            {
                for (const editor of editors) { this.render(editor); }
            }),
            vscode.workspace.onDidChangeConfiguration(event =>
            {
                if (!event.affectsConfiguration('shelly-shell.bracketColorization')
                    && !event.affectsConfiguration('editor.semanticHighlighting.enabled'))
                { return; }
                for (const editor of vscode.window.visibleTextEditors) { this.render(editor); }
            })
        ];
    }

    accept(document: vscode.TextDocument, version: number, tokens: vscode.SemanticTokens,
           legend: readonly string[]): void
    {
        if (this.disposed || document.isClosed || document.version !== version
            || tokens.resultId !== String(version)) { return; }
        const delimiter = legend.indexOf('shellyDelimiter');
        const colors: vscode.Range[][] = Array.from({ length: 4 }, () => []);
        const stack: string[] = [];
        let line = 0;
        let column = 0;
        let textLine = -1;
        let text = '';
        const data = tokens.data;
        for (let index = 0; index + 4 < data.length; index += 5)
        {
            line += data[index];
            column = data[index] ? data[index + 1] : column + data[index + 1];
            if (data[index + 3] !== delimiter || line >= document.lineCount) { continue; }
            if (textLine !== line) { text = document.lineAt(line).text; textLine = line; }
            const end = column + data[index + 2];
            if (end > text.length) { continue; }
            // Semantic positions and JS string indices both count UTF-16 code units.
            for (let offset = column; offset < end; offset++)
            {
                const character = text[offset];
                const opening = '([{'.indexOf(character);
                const closing = ')]}'.indexOf(character);
                if (opening < 0 && closing < 0) { continue; }
                let color: number;
                if (opening >= 0)
                {
                    color = stack.length % 3;
                    stack.push(')]}'[opening]);
                }
                else if (stack.length && stack[stack.length - 1] === character)
                {
                    stack.pop();
                    color = stack.length % 3;
                }
                else { color = 3; }
                colors[color].push(new vscode.Range(line, offset, line, offset + 1));
            }
        }
        this.documents.set(document.uri.toString(), { version, colors });
        for (const editor of vscode.window.visibleTextEditors)
        {
            if (editor.document.uri.toString() === document.uri.toString()) { this.render(editor); }
        }
    }

    private forget(document: vscode.TextDocument): void
    {
        this.documents.delete(document.uri.toString());
        for (const editor of vscode.window.visibleTextEditors)
        {
            if (editor.document.uri.toString() === document.uri.toString()) { this.render(editor); }
        }
    }

    private render(editor: vscode.TextEditor): void
    {
        if (this.disposed) { return; }
        const cached = this.documents.get(editor.document.uri.toString());
        const enabled = editor.document.languageId === 'shelly'
            && vscode.workspace.getConfiguration('shelly-shell', editor.document)
                .get<boolean>('bracketColorization', true)
            && vscode.workspace.getConfiguration('editor', editor.document)
                .get<boolean | string>('semanticHighlighting.enabled') !== false;
        const colors = enabled && cached?.version === editor.document.version
            ? cached.colors : undefined;
        for (let index = 0; index < this.decorations.length; index++)
        { editor.setDecorations(this.decorations[index], colors?.[index] ?? []); }
    }

    dispose(): void
    {
        if (this.disposed) { return; }
        this.disposed = true;
        this.documents.clear();
        for (const subscription of this.subscriptions) { subscription.dispose(); }
        for (const decoration of this.decorations) { decoration.dispose(); }
    }
}
