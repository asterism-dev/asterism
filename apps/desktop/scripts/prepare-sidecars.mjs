// Builds the daemon and CLI and copies them where Tauri's externalBin expects them.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const release = process.argv.includes('--release');
const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const binaries = join(root, 'apps', 'desktop', 'src-tauri', 'binaries');

execFileSync('cargo', ['build', '-p', 'asterism', '--bins', ...(release ? ['--release'] : [])], {
  cwd: root,
  stdio: 'inherit',
});
const triple = execFileSync('rustc', ['--print', 'host-tuple']).toString().trim();
mkdirSync(binaries, { recursive: true });
for (const bin of ['asterismd', 'asterism', 'asterism-plugin-github']) {
  copyFileSync(join(root, 'target', release ? 'release' : 'debug', bin), join(binaries, `${bin}-${triple}`));
}
