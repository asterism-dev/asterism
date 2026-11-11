import {
  File,
  FileArchive,
  FileCode,
  FileCog,
  FileImage,
  FileJson,
  FileLock,
  FileSpreadsheet,
  FileTerminal,
  FileText,
} from '@lucide/vue';
import type { Component } from 'vue';

export interface FileIcon {
  icon: Component;
  color?: string;
}

const code = (color: string): FileIcon => ({ icon: FileCode, color });
const config: FileIcon = { icon: FileCog, color: '#8a8f98' };

const BY_EXT: Record<string, FileIcon> = {
  rs: code('#dea584'),
  ts: code('#3178c6'),
  tsx: code('#3178c6'),
  js: code('#e5c07b'),
  mjs: code('#e5c07b'),
  cjs: code('#e5c07b'),
  jsx: code('#e5c07b'),
  vue: code('#42b883'),
  py: code('#4b8bbe'),
  go: code('#00add8'),
  java: code('#e76f00'),
  kt: code('#a97bff'),
  swift: code('#f05138'),
  c: code('#7f9cc4'),
  h: code('#7f9cc4'),
  cpp: code('#f34b7d'),
  rb: code('#cc342d'),
  php: code('#777bb4'),
  html: code('#e34c26'),
  css: code('#663399'),
  scss: code('#c6538c'),
  proto: code('#8a8f98'),
  sql: code('#e38c00'),
  sh: { icon: FileTerminal, color: '#89e051' },
  bash: { icon: FileTerminal, color: '#89e051' },
  zsh: { icon: FileTerminal, color: '#89e051' },
  json: { icon: FileJson, color: '#e5c07b' },
  md: { icon: FileText, color: '#519aba' },
  txt: { icon: FileText },
  toml: config,
  yaml: config,
  yml: config,
  ini: config,
  cfg: config,
  env: config,
  lock: { icon: FileLock, color: '#8a8f98' },
  csv: { icon: FileSpreadsheet, color: '#4caf50' },
  png: { icon: FileImage, color: '#a074c4' },
  jpg: { icon: FileImage, color: '#a074c4' },
  jpeg: { icon: FileImage, color: '#a074c4' },
  gif: { icon: FileImage, color: '#a074c4' },
  svg: { icon: FileImage, color: '#ffb13b' },
  webp: { icon: FileImage, color: '#a074c4' },
  ico: { icon: FileImage, color: '#a074c4' },
  zip: { icon: FileArchive },
  gz: { icon: FileArchive },
  tar: { icon: FileArchive },
};

const BY_NAME: Record<string, FileIcon> = {
  dockerfile: config,
  makefile: { icon: FileTerminal, color: '#89e051' },
  '.gitignore': config,
  '.gitattributes': config,
  '.editorconfig': config,
  'package-lock.json': BY_EXT.lock,
  'cargo.lock': BY_EXT.lock,
};

export function fileIcon(name: string): FileIcon {
  const lower = name.toLowerCase();
  const dot = lower.lastIndexOf('.');
  return BY_NAME[lower] ?? (dot >= 0 ? BY_EXT[lower.slice(dot + 1)] : undefined) ?? { icon: File };
}
