import { describe, expect, it } from 'vitest';
import { parseFindings, positionAtByte } from '../lint-findings';

describe('parseFindings', () => {
  it('parses a lint JSON array', () => {
    const json = JSON.stringify([
      {
        path: 'a.mc',
        line: 2,
        column: 5,
        severity: 'warning',
        code: 'unused-variable',
        message: 'unused variable `x`',
        span: { start: 10, end: 11 },
        notes: [],
        help: null,
        suggestions: [],
      },
    ]);
    const findings = parseFindings(json);
    expect(findings).toHaveLength(1);
    expect(findings[0].code).toBe('unused-variable');
    expect(findings[0].line).toBe(2);
  });

  it('returns [] for empty array, invalid JSON, and non-array', () => {
    expect(parseFindings('[]')).toEqual([]);
    expect(parseFindings('not json')).toEqual([]);
    expect(parseFindings('{"a":1}')).toEqual([]);
  });

  it('drops entries without a message', () => {
    expect(parseFindings('[{"code":"x"},{"message":"ok","code":"y"}]')).toHaveLength(1);
  });
});

describe('positionAtByte', () => {
  it('maps ASCII offsets', () => {
    const src = 'ab\ncd\nef';
    // bytes: a=0 b=1 \n=2 c=3 d=4 \n=5 e=6 f=7
    expect(positionAtByte(src, 0)).toEqual({ line: 0, character: 0 });
    expect(positionAtByte(src, 2)).toEqual({ line: 0, character: 2 }); // the '\n'
    expect(positionAtByte(src, 3)).toEqual({ line: 1, character: 0 });
    expect(positionAtByte(src, 5)).toEqual({ line: 1, character: 2 }); // the '\n'
    expect(positionAtByte(src, 6)).toEqual({ line: 2, character: 0 });
    expect(positionAtByte(src, 8)).toEqual({ line: 2, character: 2 }); // end
  });

  it('counts UTF-16 code units for non-ASCII', () => {
    // 'é' is 2 UTF-8 bytes, 1 UTF-16 unit; '😀' is 4 UTF-8 bytes, 2 UTF-16 units.
    const src = 'é😀x';
    expect(positionAtByte(src, 0)).toEqual({ line: 0, character: 0 });
    expect(positionAtByte(src, 2)).toEqual({ line: 0, character: 1 }); // after é
    expect(positionAtByte(src, 6)).toEqual({ line: 0, character: 3 }); // 'x'
    expect(positionAtByte(src, 7)).toEqual({ line: 0, character: 4 }); // end
  });

  it('clamps out-of-range offsets', () => {
    const src = 'ab';
    expect(positionAtByte(src, 999)).toEqual({ line: 0, character: 2 });
    expect(positionAtByte(src, -5)).toEqual({ line: 0, character: 0 });
  });

  it('handles CRLF', () => {
    const src = 'ab\r\ncd';
    // bytes: a=0 b=1 \r=2 \n=3 c=4 d=5
    expect(positionAtByte(src, 4)).toEqual({ line: 1, character: 0 });
    expect(positionAtByte(src, 5)).toEqual({ line: 1, character: 1 });
  });
});
