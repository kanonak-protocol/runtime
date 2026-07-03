/**
 * kanonak-wire — the TypeScript port of the Kanonak wire kernel
 * (`kanonak.org/wire-form`, `wireFormatVersion "1"`).
 *
 * A minimal, allocation-conscious binary reader/writer for hot-path wire
 * protocols. Generated protocol codecs call this kernel; it contains only what
 * is invariant across ALL protocols — bounds-checked cursor reads/writes,
 * big-endian integers, strict text validation, and a rich error taxonomy.
 *
 * Zero-copy contract: `bytes(n)` and `rest()` return subarray VIEWS into the
 * source buffer, never copies. Fail-loud contract: no null returns, no partial
 * values, no lossy decodes — every failure is a `WireError` stating what was
 * expected, what was found, and where.
 */

export const WIRE_FORMAT_VERSION = '1';

export type WireErrorKind =
  | 'Truncated'
  | 'LengthOverrun'
  | 'TrailingBytes'
  | 'InvalidUtf8'
  | 'InvalidUuid'
  | 'ValueOutOfRange'
  | 'UnknownTag';

export class WireError extends Error {
  readonly kind: WireErrorKind;
  /** Absolute byte offset where the failing read started (read-side errors). */
  readonly offset?: number;

  constructor(kind: WireErrorKind, message: string, offset?: number) {
    super(message);
    this.name = 'WireError';
    this.kind = kind;
    this.offset = offset;
  }

  static truncated(needed: number, remaining: number, offset: number, context: string): WireError {
    return new WireError(
      'Truncated',
      `Truncated: ${context} needs ${needed} byte(s) at offset ${offset}, ${remaining} remain`,
      offset,
    );
  }

  static lengthOverrun(declared: number, remaining: number, offset: number, context: string): WireError {
    return new WireError(
      'LengthOverrun',
      `LengthOverrun: ${context} at offset ${offset} declares ${declared} byte(s), ${remaining} remain after the length field`,
      offset,
    );
  }

  static trailingBytes(count: number, offset: number): WireError {
    return new WireError(
      'TrailingBytes',
      `TrailingBytes: expected end of buffer at offset ${offset}, ${count} byte(s) remain`,
      offset,
    );
  }

  static invalidUtf8(offset: number | undefined, context: string): WireError {
    return new WireError(
      'InvalidUtf8',
      offset === undefined
        ? `InvalidUtf8: ${context}`
        : `InvalidUtf8: ${context} at offset ${offset}`,
      offset,
    );
  }

  static invalidUuid(context: string): WireError {
    return new WireError('InvalidUuid', `InvalidUuid: ${context}`);
  }

  static valueOutOfRange(value: number, type: string): WireError {
    return new WireError('ValueOutOfRange', `ValueOutOfRange: ${value} is not a valid ${type}`);
  }

  /** Constructor for generated union dispatch on an unrecognized tag byte. */
  static unknownTag(tag: number, context: string): WireError {
    return new WireError('UnknownTag', `UnknownTag: 0x${tag.toString(16).padStart(2, '0')} is not a known ${context}`);
  }
}

const strictUtf8Decoder = new TextDecoder('utf-8', { fatal: true });
const utf8Encoder = new TextEncoder();

const HEX: string[] = [];
for (let i = 0; i < 256; i++) HEX.push(i.toString(16).padStart(2, '0'));

function uuidFromBytes(b: Uint8Array): string {
  let h = '';
  for (let i = 0; i < 16; i++) h += HEX[b[i]];
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

const UUID_RE = /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;

/** True iff the UTF-16 string has no unpaired surrogates. */
function isWellFormedUtf16(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const d = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
      if (d < 0xdc00 || d > 0xdfff) return false;
      i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      return false;
    }
  }
  return true;
}

/** A bounds-checked cursor over an immutable byte buffer. Never copies. */
export class WireReader {
  private readonly buf: Uint8Array;
  private pos = 0;

  constructor(buf: Uint8Array) {
    this.buf = buf;
  }

  private need(n: number, context: string): void {
    if (this.buf.length - this.pos < n) {
      throw WireError.truncated(n, this.buf.length - this.pos, this.pos, context);
    }
  }

  u8(): number {
    this.need(1, 'u8');
    return this.buf[this.pos++];
  }

  u16be(): number {
    this.need(2, 'u16be');
    const v = (this.buf[this.pos] << 8) | this.buf[this.pos + 1];
    this.pos += 2;
    return v;
  }

  u32be(): number {
    this.need(4, 'u32be');
    const b = this.buf;
    const p = this.pos;
    const v = ((b[p] << 24) | (b[p + 1] << 16) | (b[p + 2] << 8) | b[p + 3]) >>> 0;
    this.pos += 4;
    return v;
  }

  /** Exactly n bytes as a zero-copy subarray view. */
  bytes(n: number): Uint8Array {
    this.need(n, `bytes(${n})`);
    const v = this.buf.subarray(this.pos, this.pos + n);
    this.pos += n;
    return v;
  }

  /** 16 bytes as a lowercase hyphenated UUID string. Any 16 bytes are legal. */
  uuid(): string {
    this.need(16, 'uuid');
    const v = uuidFromBytes(this.buf.subarray(this.pos, this.pos + 16));
    this.pos += 16;
    return v;
  }

  /** n bytes decoded as STRICT UTF-8. Bounds are checked before validity. */
  utf8(n: number): string {
    const start = this.pos;
    const view = this.bytes(n);
    try {
      return strictUtf8Decoder.decode(view);
    } catch {
      this.pos = start; // the read did not take effect
      throw WireError.invalidUtf8(start, `utf8(${n})`);
    }
  }

  /** u16be length L, then exactly L bytes (zero-copy view). */
  lenPrefixedBytes16(): Uint8Array {
    const start = this.pos;
    this.need(2, 'lenPrefixedBytes16');
    const declared = (this.buf[this.pos] << 8) | this.buf[this.pos + 1];
    const remainingAfterLength = this.buf.length - this.pos - 2;
    if (declared > remainingAfterLength) {
      throw WireError.lengthOverrun(declared, remainingAfterLength, start, 'lenPrefixedBytes16');
    }
    this.pos += 2;
    const v = this.buf.subarray(this.pos, this.pos + declared);
    this.pos += declared;
    return v;
  }

  /** All remaining bytes (possibly empty) as a zero-copy view. Never errors. */
  rest(): Uint8Array {
    const v = this.buf.subarray(this.pos);
    this.pos = this.buf.length;
    return v;
  }

  remaining(): number {
    return this.buf.length - this.pos;
  }

  expectEnd(): void {
    const count = this.buf.length - this.pos;
    if (count > 0) {
      throw WireError.trailingBytes(count, this.pos);
    }
  }
}

/** An append-only buffer builder with validated writes. */
export class WireWriter {
  private buf: Uint8Array;
  private len = 0;

  constructor(capacity = 64) {
    this.buf = new Uint8Array(capacity < 1 ? 1 : capacity);
  }

  static withCapacity(capacity: number): WireWriter {
    return new WireWriter(capacity);
  }

  private grow(add: number): void {
    if (this.len + add <= this.buf.length) return;
    let cap = this.buf.length * 2;
    while (cap < this.len + add) cap *= 2;
    const next = new Uint8Array(cap);
    next.set(this.buf.subarray(0, this.len));
    this.buf = next;
  }

  private uint(value: number, max: number, type: string): void {
    if (!Number.isInteger(value) || value < 0 || value > max) {
      throw WireError.valueOutOfRange(value, type);
    }
  }

  u8(value: number): this {
    this.uint(value, 0xff, 'u8');
    this.grow(1);
    this.buf[this.len++] = value;
    return this;
  }

  u16be(value: number): this {
    this.uint(value, 0xffff, 'u16be');
    this.grow(2);
    this.buf[this.len++] = value >>> 8;
    this.buf[this.len++] = value & 0xff;
    return this;
  }

  u32be(value: number): this {
    this.uint(value, 0xffffffff, 'u32be');
    this.grow(4);
    this.buf[this.len++] = value >>> 24;
    this.buf[this.len++] = (value >>> 16) & 0xff;
    this.buf[this.len++] = (value >>> 8) & 0xff;
    this.buf[this.len++] = value & 0xff;
    return this;
  }

  bytes(b: Uint8Array): this {
    this.grow(b.length);
    this.buf.set(b, this.len);
    this.len += b.length;
    return this;
  }

  /** Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes. */
  uuid(s: string): this {
    if (!UUID_RE.test(s)) {
      throw WireError.invalidUuid(`"${s}" is not a hyphenated 8-4-4-4-12 UUID`);
    }
    const hex = s.replace(/-/g, '');
    this.grow(16);
    for (let i = 0; i < 16; i++) {
      this.buf[this.len++] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return this;
  }

  /** UTF-8 encode. Unpaired surrogates are InvalidUtf8 — never U+FFFD. */
  utf8(s: string): this {
    if (!isWellFormedUtf16(s)) {
      throw WireError.invalidUtf8(undefined, 'string contains an unpaired surrogate');
    }
    return this.bytes(utf8Encoder.encode(s));
  }

  /** u16be length, then the bytes. Length above 0xFFFF is ValueOutOfRange. */
  lenPrefixedBytes16(b: Uint8Array): this {
    if (b.length > 0xffff) {
      throw WireError.valueOutOfRange(b.length, 'lenPrefixedBytes16 length');
    }
    this.u16be(b.length);
    return this.bytes(b);
  }

  /** The written bytes, exact length (a copy — the builder stays reusable). */
  toBytes(): Uint8Array {
    return this.buf.slice(0, this.len);
  }
}
