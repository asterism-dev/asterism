export interface FileLink {
  start: number;
  end: number;
  path: string;
  line?: number;
}

// Group 1 is the boundary before a token; requiring one keeps tokens inside URLs (`https://…`) out.
const TOKEN = /(^|[\s(["'`])((?:~\/|\.{1,2}\/|\/)?[\w.@+-]+(?:\/[\w.@+-]+)*)(?::(\d+)(?::\d+)?)?/g;
const PREFIX = /^(?:~\/|\.{1,2}\/|\/)/;
const EXTENSION = /\.[A-Za-z][\w-]*$/;

export function findFileLinks(text: string): FileLink[] {
  const links: FileLink[] = [];
  for (const match of text.matchAll(TOKEN)) {
    const [whole, boundary, token, line] = match;
    const path = token.replace(/\.+$/, '');
    if (!PREFIX.test(path) && !EXTENSION.test(path.slice(path.lastIndexOf('/') + 1))) continue;
    const start = match.index + boundary.length;
    // A trailing dot ends a sentence, so any suffix after it is not this path's line.
    const trimmed = path !== token;
    const end = trimmed ? start + path.length : match.index + whole.length;
    const lineNumber = !trimmed && line ? Number(line) : 0;
    links.push(lineNumber > 0 ? { start, end, path, line: lineNumber } : { start, end, path });
  }
  return links;
}
