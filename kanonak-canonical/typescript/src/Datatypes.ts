/**
 * Datatype carriers + canonical lexical forms — the normative heart of the
 * canonical form. This is the part every language port of `kanonak-canonical`
 * (TS, C#, Rust, Go, Java, Python) must reproduce byte-for-byte, so it is
 * written as pure functions with no dependence on a parser, object model, or
 * runtime numeric coercion.
 *
 * Identity is `(carrier, canonicalLexical)`. A literal's datatype is part of its
 * identity: derived datatypes that share a VALUE SPACE collapse onto one carrier
 * (every `xsd:*Integer`/`xsd:long`/… is the Integer carrier), but distinct value
 * spaces stay distinct by tag (`xsd:decimal 5` != `xsd:integer 5`).
 *
 * STABILITY CONTRACT: this module is FROZEN to `canonicalFormVersion` "1" — its
 * output bytes are permanent content addresses. Changing any rule below means a
 * NEW canonical-form version, never an edit in place. Depends only on the
 * coordinate module (pure functions, vector-pinned) — no parser, no object
 * model, no runtime numeric coercion.
 */
import { lenientVersionlessKey } from './Coordinate.js';

/** The closed set of canonical-form carriers (v1). */
export enum Carrier {
  Integer = 'integer',
  Decimal = 'decimal',
  Double = 'double',
  Float = 'float',
  Boolean = 'boolean',
  String = 'string',
  AnyUri = 'anyURI',
  LangString = 'langString',
  DateTime = 'dateTime',
  Date = 'date',
  Time = 'time',
  HexBinary = 'hexBinary',
  Base64Binary = 'base64Binary',
}

/**
 * Normative datatype-URI -> carrier routing. The whole integer-derivation tree
 * routes to Integer; `normalizedString`/`token` route to String (whitespace is a
 * facet, not value canonicalization).
 */
const XSD_CARRIER: ReadonlyArray<[string, Carrier]> = [
  ['integer', Carrier.Integer],
  ['long', Carrier.Integer], ['int', Carrier.Integer], ['short', Carrier.Integer], ['byte', Carrier.Integer],
  ['unsignedLong', Carrier.Integer], ['unsignedInt', Carrier.Integer],
  ['unsignedShort', Carrier.Integer], ['unsignedByte', Carrier.Integer],
  ['nonNegativeInteger', Carrier.Integer], ['positiveInteger', Carrier.Integer],
  ['nonPositiveInteger', Carrier.Integer], ['negativeInteger', Carrier.Integer],
  ['decimal', Carrier.Decimal],
  ['double', Carrier.Double],
  ['float', Carrier.Float],
  ['boolean', Carrier.Boolean],
  ['string', Carrier.String],
  ['normalizedString', Carrier.String],
  ['token', Carrier.String],
  ['anyURI', Carrier.AnyUri],
  ['dateTime', Carrier.DateTime],
  ['date', Carrier.Date],
  ['time', Carrier.Time],
  ['hexBinary', Carrier.HexBinary],
  ['base64Binary', Carrier.Base64Binary],
];

const CARRIER_BY_URI: ReadonlyMap<string, Carrier> = new Map(
  XSD_CARRIER.map(([name, carrier]) => [`kanonak.org/core-xsd/${name}`, carrier]),
);
const LANG_STRING_KEY = 'kanonak.org/core-rdf/langString';

/**
 * The carrier for a datatype URI, or `undefined` if outside the v1 set. An
 * out-of-set OR structurally unparseable datatype is canonicalized as a
 * byte-preserved raw token, never guessed into a carrier — which is why the
 * routing key comes from the total `lenientVersionlessKey`, not the throwing
 * parser (pinned by the lenientKeyVectors in coordinate-vectors.json).
 */
export function carrierOf(datatypeUri: string): Carrier | undefined {
  const key = lenientVersionlessKey(datatypeUri);
  if (key === undefined) return undefined;
  if (key === LANG_STRING_KEY) return Carrier.LangString;
  return CARRIER_BY_URI.get(key);
}

// --- Integer carrier — arbitrary precision (BigInt-backed). -----------------

/** Canonical `xsd:integer`: optional `-`, no leading zeros, `0` for zero, never `-0`/`+`. */
export function canonicalInteger(raw: string): string {
  const token = raw.trim();
  if (!/^[+-]?\d+$/.test(token)) {
    throw new Error(`canonicalInteger: '${raw}' is not a valid xsd:integer lexical`);
  }
  return BigInt(token.replace(/^\+/, '')).toString();
}

// --- Decimal carrier — arbitrary precision (string arithmetic, no double). ---

/** Canonical `xsd:decimal`: minimal value form; trailing-zero/`-0` collapse. */
export function canonicalDecimal(raw: string): string {
  const token = raw.trim();
  const m = /^([+-]?)(\d*)(?:\.(\d*))?$/.exec(token);
  if (!m || (m[2] === '' && (m[3] ?? '') === '')) {
    throw new Error(`canonicalDecimal: '${raw}' is not a valid xsd:decimal lexical`);
  }
  const sign = m[1] === '-' ? '-' : '';
  const intPart = (m[2] ?? '').replace(/^0+/, '') || '0';
  const fracPart = (m[3] ?? '').replace(/0+$/, '');
  const magnitude = fracPart.length > 0 ? `${intPart}.${fracPart}` : intPart;
  if (magnitude === '0') return '0';
  return `${sign}${magnitude}`;
}

// --- IEEE carriers — shortest round-tripping decimal (RFC 8785). ------------

function canonicalIeee(raw: string, round: (n: number) => number, label: string): string {
  const token = raw.trim();
  if (token === 'NaN') return 'NaN';
  if (token === 'INF') return 'INF';
  if (token === '-INF') return '-INF';
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(token)) {
    throw new Error(`canonical${label}: '${raw}' is not a valid xsd:${label.toLowerCase()} lexical`);
  }
  const n = round(Number(token));
  if (!Number.isFinite(n)) {
    throw new Error(`canonical${label}: '${raw}' is out of the finite range`);
  }
  if (Object.is(n, -0)) return '0';
  return String(n);
}

/** Canonical `xsd:double` — shortest round-tripping decimal; INF / -INF / NaN. */
export function canonicalDouble(raw: string): string {
  return canonicalIeee(raw, (n) => n, 'Double');
}

/** Canonical `xsd:float` — like double but rounded to IEEE single (`Math.fround`). */
export function canonicalFloat(raw: string): string {
  return canonicalIeee(raw, (n) => Math.fround(n), 'Float');
}

// --- Boolean carrier. -------------------------------------------------------

/** Canonical `xsd:boolean`: `true` / `false` (accepts `1`/`0`). */
export function canonicalBoolean(raw: string): string {
  const token = raw.trim();
  if (token === 'true' || token === '1') return 'true';
  if (token === 'false' || token === '0') return 'false';
  throw new Error(`canonicalBoolean: '${raw}' is not a valid xsd:boolean lexical`);
}

// --- String carriers — Unicode NFC, no other normalization. -----------------

/** Canonical string value: Unicode NFC. */
export function canonicalString(raw: string): string {
  return raw.normalize('NFC');
}

/** Canonical `xsd:anyURI`: NFC (distinct tag from string, same lexical rule). */
export function canonicalAnyUri(raw: string): string {
  return raw.normalize('NFC');
}

/** Canonical `rdf:langString`: NFC value + BCP 47 canonical-case language tag. */
export function canonicalLangString(value: string, lang: string): { value: string; lang: string } {
  return { value: value.normalize('NFC'), lang: canonicalLanguageTag(lang) };
}

/** BCP 47 canonical case by subtag position. */
export function canonicalLanguageTag(tag: string): string {
  const subtags = tag.trim().split('-');
  if (subtags.length === 0 || subtags[0] === '') {
    throw new Error(`canonicalLanguageTag: '${tag}' is not a valid language tag`);
  }
  return subtags
    .map((sub, i) => {
      if (i === 0) return sub.toLowerCase();
      if (sub.length === 4 && /^[A-Za-z]{4}$/.test(sub)) {
        return sub[0].toUpperCase() + sub.slice(1).toLowerCase();
      }
      if (sub.length === 2 && /^[A-Za-z]{2}$/.test(sub)) return sub.toUpperCase();
      return sub.toLowerCase();
    })
    .join('-');
}

// --- Binary carriers. -------------------------------------------------------

/** Canonical `xsd:hexBinary`: uppercase hex digits, even length. */
export function canonicalHexBinary(raw: string): string {
  const token = raw.trim();
  if (!/^([0-9A-Fa-f]{2})*$/.test(token)) {
    throw new Error(`canonicalHexBinary: '${raw}' is not a valid xsd:hexBinary lexical`);
  }
  return token.toUpperCase();
}

/** Canonical `xsd:base64Binary`: RFC 4648, canonical padding, no line breaks. */
export function canonicalBase64(raw: string): string {
  const stripped = raw.replace(/\s+/g, '');
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(stripped) || stripped.length % 4 !== 0) {
    throw new Error(`canonicalBase64: '${raw}' is not a valid xsd:base64Binary lexical`);
  }
  // Decode then re-encode (pure JS, no `Buffer`, so this runs in any JS
  // environment): collapses any padding/line-wrap variant to the one canonical
  // RFC 4648 encoding and zeroes a final quantum's unused bits — byte-identical
  // to a native base64 round-trip.
  return base64Encode(base64Decode(stripped));
}

// --- base64 (RFC 4648 standard alphabet), pure JS, no `Buffer`. -------------

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

/** Decode a validated standard-alphabet base64 string to bytes. */
function base64Decode(s: string): Uint8Array {
  const clean = s.replace(/=+$/, '');
  const out = new Uint8Array((clean.length * 6) >> 3);
  let bits = 0;
  let nbits = 0;
  let oi = 0;
  for (let i = 0; i < clean.length; i++) {
    bits = (bits << 6) | B64.indexOf(clean[i]);
    nbits += 6;
    if (nbits >= 8) {
      nbits -= 8;
      out[oi++] = (bits >> nbits) & 0xff;
    }
  }
  return out;
}

/** Encode bytes to standard-alphabet base64 with canonical `=` padding. */
function base64Encode(bytes: Uint8Array): string {
  let out = '';
  let i = 0;
  for (; i + 3 <= bytes.length; i += 3) {
    const n = (bytes[i] << 16) | (bytes[i + 1] << 8) | bytes[i + 2];
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + B64[n & 63];
  }
  const rem = bytes.length - i;
  if (rem === 1) {
    const n = bytes[i] << 16;
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + '==';
  } else if (rem === 2) {
    const n = (bytes[i] << 16) | (bytes[i + 1] << 8);
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + '=';
  }
  return out;
}

// --- Temporal carriers — dateTime / date / time, by XSD 1.1's rules. ----------
//
// The proleptic Gregorian calendar with a year 0, as XSD 1.1's. A year of any length is an exact integer, so a
// UTC shift that crosses a year boundary carries into it without a range limit (runtime#36).

const pad2 = (n: number): string => (n < 10 ? `0${n}` : `${n}`);

const DAYS_IN_MONTH = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

function isLeapYear(year: bigint): boolean {
  const r = ((year % BigInt(400)) + BigInt(400)) % BigInt(400);
  return r % BigInt(4) === BigInt(0) && (r % BigInt(100) !== BigInt(0) || r === BigInt(0));
}

function daysInMonth(year: bigint, month: number): number {
  return month === 2 && isLeapYear(year) ? 29 : DAYS_IN_MONTH[month - 1];
}

/** A year at least four digits wide, its sign kept; year 0 has none. */
function yearText(year: bigint): string {
  const digits = (year < BigInt(0) ? -year : year).toString().padStart(4, '0');
  return year < BigInt(0) ? `-${digits}` : digits;
}

interface Civil { year: bigint; month: number; day: number }

function civilOf(yy: string, mo: string, dd: string, what: string, raw: string): Civil {
  const year = BigInt(yy);
  const month = Number(mo);
  const day = Number(dd);
  if (month < 1 || month > 12) throw new Error(`${what}: '${raw}' has month ${mo}; a month is 01 to 12`);
  if (day < 1 || day > daysInMonth(year, month)) {
    throw new Error(`${what}: '${raw}' has day ${dd}, which month ${mo} of year ${yearText(year)} doesn't have`);
  }
  return { year, month, day };
}

function nextDay(c: Civil): Civil {
  if (c.day < daysInMonth(c.year, c.month)) return { ...c, day: c.day + 1 };
  if (c.month < 12) return { year: c.year, month: c.month + 1, day: 1 };
  return { year: c.year + BigInt(1), month: 1, day: 1 };
}

function previousDay(c: Civil): Civil {
  if (c.day > 1) return { ...c, day: c.day - 1 };
  if (c.month > 1) return { year: c.year, month: c.month - 1, day: daysInMonth(c.year, c.month - 1) };
  return { year: c.year - BigInt(1), month: 12, day: 31 };
}

/** Minutes east of UTC, or undefined with no offset. XSD 1.1 bounds an offset at 14:00. */
function offsetMinutes(tz: string | undefined, what: string, raw: string): number | undefined {
  if (!tz) return undefined;
  if (tz === 'Z') return 0;
  const hours = Number(tz.slice(1, 3));
  const minutes = Number(tz.slice(4, 6));
  if (minutes > 59 || hours > 14 || (hours === 14 && minutes > 0)) {
    throw new Error(`${what}: '${raw}' has offset ${tz}; an offset is at most 14:00 either way`);
  }
  return (tz[0] === '-' ? -1 : 1) * (hours * 60 + minutes);
}

/** XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day. */
function clockOf(hh: string, mi: string, ss: string, fraction: string, what: string, raw: string): { hour: number; minute: number } {
  const hour = Number(hh);
  const minute = Number(mi);
  const second = Number(ss);
  const endOfDay = hour === 24 && minute === 0 && second === 0 && fraction === '';
  if (minute > 59 || second > 59 || (hour > 23 && !endOfDay)) {
    throw new Error(`${what}: '${raw}' has time ${hh}:${mi}:${ss}${fraction}; XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly`);
  }
  return { hour, minute };
}

function canonicalFraction(frac: string | undefined): string {
  if (!frac) return '';
  const trimmed = frac.replace(/^\./, '').replace(/0+$/, '');
  return trimmed.length > 0 ? `.${trimmed}` : '';
}

function canonicalTz(tz: string | undefined): string {
  if (!tz) return '';
  if (tz === 'Z' || tz === '+00:00' || tz === '-00:00') return 'Z';
  return tz;
}

const DATETIME_RE = /^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;
const DATE_RE = /^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$/;
const TIME_RE = /^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;

/** Canonical `xsd:dateTime`. Offset-bearing -> UTC `Z`; timezone-less -> floating; 24:00:00 -> next day. */
export function canonicalDateTime(raw: string): string {
  const what = 'canonicalDateTime';
  const m = DATETIME_RE.exec(raw.trim());
  if (!m) throw new Error(`${what}: '${raw}' is not a valid xsd:dateTime lexical`);
  const [, yy, mo, dd, hh, mi, ss, frac, tz] = m;
  const fraction = canonicalFraction(frac);
  let civil = civilOf(yy, mo, dd, what, raw);
  const clock = clockOf(hh, mi, ss, fraction, what, raw);
  let minutes = clock.hour * 60 + clock.minute - (offsetMinutes(tz, what, raw) ?? 0);
  if (minutes < 0) {
    minutes += 1440;
    civil = previousDay(civil);
  } else if (minutes >= 1440) {
    minutes -= 1440;
    civil = nextDay(civil);
  }
  const date = `${yearText(civil.year)}-${pad2(civil.month)}-${pad2(civil.day)}`;
  return `${date}T${pad2(Math.floor(minutes / 60))}:${pad2(minutes % 60)}:${ss}${fraction}${tz ? 'Z' : ''}`;
}

/** Canonical `xsd:date`: lexical only — canonical year width + tz spelling, NO UTC shift. */
export function canonicalDate(raw: string): string {
  const what = 'canonicalDate';
  const m = DATE_RE.exec(raw.trim());
  if (!m) throw new Error(`${what}: '${raw}' is not a valid xsd:date lexical`);
  const [, yy, mo, dd, tz] = m;
  const civil = civilOf(yy, mo, dd, what, raw);
  offsetMinutes(tz, what, raw);
  return `${yearText(civil.year)}-${mo}-${dd}${canonicalTz(tz)}`;
}

/** Canonical `xsd:time`: lexical only; `24:00:00` -> `00:00:00`. */
export function canonicalTime(raw: string): string {
  const what = 'canonicalTime';
  const m = TIME_RE.exec(raw.trim());
  if (!m) throw new Error(`${what}: '${raw}' is not a valid xsd:time lexical`);
  const [, hh, mi, ss, frac, tz] = m;
  const fraction = canonicalFraction(frac);
  const clock = clockOf(hh, mi, ss, fraction, what, raw);
  offsetMinutes(tz, what, raw);
  return `${clock.hour === 24 ? '00' : hh}:${mi}:${ss}${fraction}${canonicalTz(tz)}`;
}

// --- Dispatch — the single carrier -> canonical-lexical entry. ---------------

/** Canonical lexical form of a raw token under a carrier. Throws on malformed input. */
export function canonicalScalarLexical(carrier: Carrier, raw: string): string {
  switch (carrier) {
    case Carrier.Integer: return canonicalInteger(raw);
    case Carrier.Decimal: return canonicalDecimal(raw);
    case Carrier.Double: return canonicalDouble(raw);
    case Carrier.Float: return canonicalFloat(raw);
    case Carrier.Boolean: return canonicalBoolean(raw);
    case Carrier.String: return canonicalString(raw);
    case Carrier.AnyUri: return canonicalAnyUri(raw);
    case Carrier.LangString: throw new Error(`canonicalLangString: a language string is a structured value, an embedded { lexicalForm, language }, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '${raw}'`);
    case Carrier.DateTime: return canonicalDateTime(raw);
    case Carrier.Date: return canonicalDate(raw);
    case Carrier.Time: return canonicalTime(raw);
    case Carrier.HexBinary: return canonicalHexBinary(raw);
    case Carrier.Base64Binary: return canonicalBase64(raw);
  }
}
