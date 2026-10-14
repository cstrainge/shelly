# Publishing shelly-shell

Publisher: `cstrainge`, matching Strange Forth. Extension ID:
`cstrainge.shelly-shell`. Marketplace display name: **Shelly Shell**.
The current extension version is `0.2.1`.

The package includes the bundled client, Shelly icon, README, changelog, and MIT
license. It requires a separately installed Shelly executable supporting
`--language-server`; publish that runtime release before offering this extension.
Building requires Node.js 22 or later for the packaging tool's dependencies.

From `extensions/vscode/`:

```sh
npm ci
npm run package
code --install-extension shelly-shell.vsix
```

Before uploading, try the installed package with a current Shelly binary: open a
`.shy` file, edit declarations and command arguments, check syntax diagnostics,
and use **Shelly: Restart Language Server**. The native protocol tests live in
`tests/native/language_server.shy` and run through the repository's `test.shy`.

When ready to publish the reviewed package:

```sh
npm exec vsce -- login cstrainge
npm run "publish:marketplace"
```

`login` prompts for your existing token locally. A usable token needs Marketplace
Manage access and must still be valid. If already logged in, skip `login` and use
the stored credentials. No token belongs in these files or command
arguments. Publishing is a separate, explicit action; building and packaging do
not upload anything. You can also upload this `.vsix` through the
[publisher management page](https://marketplace.visualstudio.com/manage/publishers/cstrainge).

Microsoft's current guidance retires global Azure DevOps PATs on December 1,
2026; use its documented Entra ID publishing route when moving away from PATs.
See the [official publishing guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
for authentication and upload details.

For later releases, update `package.json`, refresh `package-lock.json`, add the
changelog entry, and rebuild the `.vsix` before publishing. The shell and extension
have independent versions.
