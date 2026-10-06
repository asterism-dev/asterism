// Builds the daemon and CLI and copies them where Tauri's externalBin expects them.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const release = process.argv.includes('--release');
const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const binaries = join(root, 'apps', 'desktop', 'src-tauri', 'binaries');

const host = execFileSync('rustc', ['--print', 'host-tuple']).toString().trim();
// Tauri sets this during `tauri build --target`; cross builds land in target/<triple>/.
const triple = process.env.TAURI_ENV_TARGET_TRIPLE ?? host;
const cross = triple !== host;

execFileSync('cargo', ['build', '-p', 'asterism', '--bins', ...(release ? ['--release'] : []), ...(cross ? ['--target', triple] : [])], {
  cwd: root,
  stdio: 'inherit',
});
mkdirSync(binaries, { recursive: true });
for (const bin of ['asterismd', 'asterism', 'asterism-plugin-claude', 'asterism-plugin-github', 'asterism-plugin-linear']) {
  copyFileSync(join(root, 'target', ...(cross ? [triple] : []), release ? 'release' : 'debug', bin), join(binaries, `${bin}-${triple}`));
}
