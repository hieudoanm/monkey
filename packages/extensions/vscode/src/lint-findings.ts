/**
 * Pure helpers for mapping `monkey lint --stdin` JSON output to editor
 * diagnostics. No `vscode` imports — unit-testable in plain Node.
 */

export interface MonkeyFinding {
  path: string;
  /** 1-based line. */
  line: number | null;
  /** 1-based column. */
  column: number | null;
  severity: 'error' | 'warning' | 'information' | 'hint' | string;
  code: string;
  message: string;
  span: { start: number; end: number } | null;
}

export interface Position {
  /** 0-based line. */
  line: number;
  /** 0-based character (UTF-16 code units, matching VS Code). */
  character: number;
}

/** Parse the JSON array emitted by `monkey lint --stdin`. Returns [] on bad input. */
export function parseFindings(json: string): MonkeyFinding[] {
  try {
    const data: unknown = JSON.parse(json);
    if (!Array.isArray(data)) return [];
    return data.filter(
      (d): d is MonkeyFinding =>
        typeof d === 'object' && d !== null && typeof (d as MonkeyFinding).message === 'string',
    );
  } catch {
    return [];
  }
}

function utf8Length(src: string): number {
  let n = 0;
  for (let i = 0; i < src.length; i += 1) {
    const code = src.charCodeAt(i);
    if (code < 0x80) n += 1;
    else if (code < 0x800) n += 2;
    else if (code >= 0xd800 && code <= 0xdbff) {
      n += 4;
      i += 1;
    } else n += 3;
  }
  return n;
}

/**
 * Convert a UTF-8 byte offset in `src` to a 0-based line/character position.
 * `character` counts UTF-16 code units, matching editor position semantics.
 * Offsets inside a multi-byte character resolve to that character's start.
 */
export function positionAtByte(src: string, byteOffset: number): Position {
  const target = Math.max(0, Math.min(byteOffset, utf8Length(src)));
  let line = 0;
  let lineStartIndex = 0;
  let consumed = 0; // UTF-8 bytes consumed before index i
  let i = 0;
  while (i < src.length) {
    const code = src.charCodeAt(i);
    const isHigh = code >= 0xd800 && code <= 0xdbff;
    const chLen = isHigh && i + 1 < src.length ? 2 : 1;
    const u8 = code === 10 ? 1 : code < 0x80 ? 1 : code < 0x800 ? 2 : isHigh ? 4 : 3;
    if (code === 10) {
      // Newline: positions at or before it stay on the current line.
      if (target <= consumed) break;
      line += 1;
      lineStartIndex = i + 1;
      consumed += 1;
      i += 1;
      continue;
    }
    if (consumed + u8 > target) break;
    consumed += u8;
    i += chLen;
  }
  return { line, character: i - lineStartIndex };
}
