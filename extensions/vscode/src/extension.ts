
import * as vscode from 'vscode';
import { homedir } from 'node:os';
import { isAbsolute, resolve } from 'node:path';
import { LanguageClient, ServerOptions } from 'vscode-languageclient/node';

import { verifyServer } from './server';


let client: LanguageClient | undefined;
let output: vscode.LogOutputChannel | undefined;
let files: vscode.FileSystemWatcher | undefined;


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
    const watcher = vscode.workspace.createFileSystemWatcher('**/*.shy');
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
        synchronize: { configurationSection: 'shelly-shell', fileEvents: watcher }
    });
    try
    {
        await next.start();
        client = next;
        files = watcher;
    }
    catch (error)
    {
        watcher.dispose();
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
    files?.dispose();
    files = undefined;
    if (previous) { await previous.dispose(); }
}
