
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { Registry, INITIAL, parseRawGrammar } = require('vscode-textmate');
const { loadWASM, OnigScanner, OnigString } = require('vscode-oniguruma');


// Use the editor's real tokenizer; Shelly owns assertions and test orchestration.
async function inspect()
{
    const input = JSON.parse(readFileSync(0, 'utf8'));
    const manifest = JSON.parse(readFileSync('package.json', 'utf8'));
    const contribution = manifest.contributes.grammars.find(item => item.language === 'shelly');
    const filename = resolve(contribution.path);
    const wasm = readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm'));
    await loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength));
    const registry = new Registry({
        onigLib: Promise.resolve({
            createOnigScanner: patterns => new OnigScanner(patterns),
            createOnigString: text => new OnigString(text)
        }),
        loadGrammar: async scope => scope === contribution.scopeName
            ? parseRawGrammar(readFileSync(filename, 'utf8'), filename) : null
    });
    try
    {
        const grammar = await registry.loadGrammarWithConfiguration(contribution.scopeName, 1, {
            balancedBracketSelectors: contribution.balancedBracketScopes,
            unbalancedBracketSelectors: contribution.unbalancedBracketScopes
        });
        const reports = input.sources.map(source =>
        {
            let state = INITIAL;
            const brackets = [];
            const excluded = [];
            for (const [line, text] of source.split(/\r\n|\r|\n/).entries())
            {
                const result = grammar.tokenizeLine2(text, state);
                state = result.ruleStack;
                for (let index = 0; index < result.tokens.length; index += 2)
                {
                    const start = result.tokens[index];
                    const end = result.tokens[index + 2] ?? text.length;
                    // VS Code's encoded metadata: standard token type in bits 8–9,
                    // balanced-bracket eligibility in bit 10. All indices are UTF-16.
                    const metadata = result.tokens[index + 1];
                    const type = (metadata >>> 8) & 3;
                    const balanced = (metadata & 1024) !== 0;
                    for (let column = start; column < end; column++)
                    {
                        const character = text[column];
                        if (!'()[]{}'.includes(character)) { continue; }
                        if (type === 0 && balanced) { brackets.push([line, column, character]); }
                        else { excluded.push([line, column, character, type, balanced]); }
                    }
                }
            }
            return { brackets, excluded };
        });
        const configuration = JSON.parse(readFileSync('language-configuration.json', 'utf8'));
        console.log(JSON.stringify({ reports, configuration,
            defaults: manifest.contributes.configurationDefaults['[shelly]'] }));
    }
    finally { registry.dispose(); }
}

inspect().catch(error => { console.error(error); process.exitCode = 1; });
