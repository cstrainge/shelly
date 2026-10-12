
import { execFile } from 'node:child_process';


/**
 * CLI help exits before shell startup. Reject older binaries before the language
 * client opens a protocol pipe that the executable cannot serve.
 */
export function verifyServer(command: string): Promise<void>
{
    return new Promise((resolve, reject) =>
        {
            execFile(command, ['--help'], { encoding: 'utf8', timeout: 2000,
                                          maxBuffer: 64 * 1024, shell: false },
                (error, stdout) =>
                {
                    if (error)
                    {
                        reject(new Error(`Cannot check Shelly executable '${command}': ` +
                                         error.message));
                    }
                    else if (!/^\s+--language-server(?:\s|$)/m.test(stdout))
                    {
                        reject(new Error(`'${command}' does not support --language-server. ` +
                            'Build a current Shelly binary and update shelly-shell.serverPath.'));
                    }
                    else { resolve(); }
                });
        });
}
