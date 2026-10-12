
import * as vscode from 'vscode';
import { homedir } from 'node:os';
import { isAbsolute, resolve } from 'node:path';
import { LanguageClient, ServerOptions } from 'vscode-languageclient/node';

import { verifyServer } from './server';
import { BracketColors } from './brackets';


let client: LanguageClient | undefined;
let output: vscode.LogOutputChannel | undefined;
let brackets: BracketColors | undefined;


function serverPath(): string
{
    const configured = vscode.workspace.getConfiguration('shelly-shell')
        .get<string>('serverPath', 'shelly').trim();
    if (!configured) { throw new Error('shelly-shell.serverPath must not be empty.'); }
    if (configured.startsWith('~/')) { return resolve(homedir(), configured.slice(2)); }
    if (isAbsolute(configured) || !/[\\/]/.test(configured)) { return configured; }
    const workspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspace) { throw new Error('A relative server path requires an open workspace.'); }
    return resolve(workspace, configured);
}


async function startServer(): Promise<void>
{
    const command = serverPath();
    output?.info(`Starting language server: ${command} --language-server`);
    await verifyServer(command);
    const colors = new BracketColors();
    const server: ServerOptions = {
        command,
        args: ['--language-server'],
        options: { shell: false }
    };
    const next = new LanguageClient('shelly-shell', 'Shelly', server, {
        outputChannel: output,
        documentSelector: [
            { language: 'shelly', scheme: 'file' },
            { language: 'shelly', scheme: 'untitled' }
        ],
        synchronize: { configurationSection: 'shelly-shell' },
        middleware: {
            provideDocumentSemanticTokens: async (document, token, provide) =>
            {
                const version = document.version;
                const result = await provide(document, token);
                const legend = next.initializeResult?.capabilities.semanticTokensProvider?.legend;
                if (result && legend && !token.isCancellationRequested)
                { colors.accept(document, version, result, legend.tokenTypes); }
                return result;
            }
        }
    });
    try
    {
        await next.start();
        client = next;
        brackets = colors;
    }
    catch (error)
    {
        colors.dispose();
        try { await next.dispose(); }
        catch (cleanup) { output?.error(`Failed to dispose starting client: ${String(cleanup)}`); }
        throw error;
    }
}


function startupFailed(error: unknown): void
{
    output?.error(String(error));
    void vscode.window.showErrorMessage(
        `Shelly language server could not start: ${String(error)} ` +
        'Check shelly-shell.serverPath and use Shelly: Restart Language Server to retry.');
}


export async function activate(context: vscode.ExtensionContext): Promise<void>
{
    output = vscode.window.createOutputChannel('Shelly', { log: true });
    context.subscriptions.push(output);
    context.subscriptions.push({ dispose: () => brackets?.dispose() });
    // The editor API accepts this rule across supported VS Code versions. The JSON
    // schema now requires an object form that older configuration loaders reject.
    context.subscriptions.push(vscode.languages.setLanguageConfiguration('shelly',
        { comments: { lineComment: '#' } }));
    context.subscriptions.push(vscode.commands.registerCommand('shelly-shell.restartServer',
        async () =>
        {
            try { await deactivate(); await startServer(); }
            catch (error) { startupFailed(error); }
        }));
    try { await startServer(); }
    catch (error) { startupFailed(error); }
}


export async function deactivate(): Promise<void>
{
    const previous = client;
    client = undefined;
    brackets?.dispose();
    brackets = undefined;
    if (previous) { await previous.dispose(); }
}
