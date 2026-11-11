import { File, FileCode, FileCog, FileLock } from '@lucide/vue';
import { describe, expect, it } from 'vitest';
import { fileIcon } from './fileIcons';

describe('fileIcon', () => {
  it('maps by exact name before extension, case-insensitively', () => {
    expect(fileIcon('Cargo.lock').icon).toBe(FileLock);
    expect(fileIcon('Makefile').color).toBeDefined();
    expect(fileIcon('.gitignore').icon).toBe(FileCog);
  });

  it('maps by extension and falls back to a plain file', () => {
    expect(fileIcon('daemon.rs')).toEqual({ icon: FileCode, color: '#dea584' });
    expect(fileIcon('README')).toEqual({ icon: File });
    expect(fileIcon('archive.unknown')).toEqual({ icon: File });
  });
});
