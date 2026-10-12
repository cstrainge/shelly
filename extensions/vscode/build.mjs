
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { build, context } from 'esbuild';


function bundledNotices(result)
{
    if (result.errors.length) { return; }
    // Preserve the actual bundled packages' license notices, including nested versions.
    const packages = new Map();
    for (const input of Object.keys(result.metafile.inputs))
    {
        if (!input.startsWith('node_modules/')) { continue; }
        let directory = dirname(resolve(input));
        while (directory !== dirname(directory))
        {
            const manifest = join(directory, 'package.json');
            if (existsSync(manifest))
            {
                const metadata = JSON.parse(readFileSync(manifest, 'utf8'));
                if (metadata.name && metadata.version)
                {
                    packages.set(`${metadata.name}@${metadata.version}`, directory);
                    break;
                }
            }
            directory = dirname(directory);
        }
    }
    const notices = [];
    for (const [name, directory] of [...packages.entries()].sort())
    {
        const license = ['LICENSE', 'LICENSE.md', 'LICENSE.txt', 'License.txt', 'license']
            .map(file => join(directory, file)).find(existsSync);
        if (!license) { throw new Error(`Missing bundled license for ${name}.`); }
        notices.push(`${name}\n${'='.repeat(name.length)}\n${readFileSync(license, 'utf8')}`);
    }
    writeFileSync('out/THIRD_PARTY_NOTICES.txt', notices.join('\n\n'));
}


// Remove stale generated assets when the language client's packaging changes.
rmSync('out', { recursive: true, force: true });

const options = {
    entryPoints: ['src/extension.ts'],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    target: 'node22',
    external: ['vscode'],
    outfile: 'out/extension.js',
    sourcemap: true,
    metafile: true,
    logLevel: 'info',
    plugins: [{
        name: 'license-notices',
        setup(builder) { builder.onEnd(bundledNotices); }
    }]
};

if (process.argv.includes('--watch'))
{
    const watcher = await context(options);
    await watcher.watch();
}
else { await build(options); }
