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
 * NEW canonical-form version, never an edit in place. Self-contained on purpose.
 */

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

/** `publisher/package/name` key from a datatype URI (version stripped). */
function datatypeKey(uri: string): string {
  const lastSlash = uri.lastIndexOf('/');
  const name = uri.slice(lastSlash + 1);
  const head = uri.slice(0, lastSlash); // publisher/package@version
  const firstSlash = head.indexOf('/');
  const publisher = head.slice(0, firstSlash);
  const pkg = head.slice(firstSlash + 1).split('@', 1)[0];
  return `${publisher}/${pkg}/${name}`;
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
 * out-of-set datatype is canonicalized as a byte-preserved raw token, never
 * guessed into a carrier.
 */
export function carrierOf(datatypeUri: string): Carrier | undefined {
  const key = datatypeKey(datatypeUri);
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
  return Buffer.from(stripped, 'base64').toString('base64');
}

// --- Temporal carriers — dateTime / date / time. ----------------------------

const pad2 = (n: number): string => (n < 10 ? `0${n}` : `${n}`);

function canonicalYear(raw: string): string {
  const neg = raw.startsWith('-');
  const digits = (neg ? raw.slice(1) : raw).replace(/^0+/, '') || '0';
  const padded = digits.length < 4 ? digits.padStart(4, '0') : digits;
  return `${neg ? '-' : ''}${padded}`;
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

function tzOffsetMinutes(tz: string): number {
  if (tz === 'Z') return 0;
  const sign = tz[0] === '-' ? -1 : 1;
  return sign * (Number(tz.slice(1, 3)) * 60 + Number(tz.slice(4, 6)));
}

const DATETIME_RE = /^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;
const DATE_RE = /^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$/;
const TIME_RE = /^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;

/** Canonical `xsd:dateTime`. Offset-bearing -> UTC `Z`; timezone-less -> floating. */
export function canonicalDateTime(raw: string): string {
  const m = DATETIME_RE.exec(raw.trim());
  if (!m) throw new Error(`canonicalDateTime: '${raw}' is not a valid xsd:dateTime lexical`);
  const [, yy, mo, dd, hh, mi, ss, frac, tz] = m;
  const fraction = canonicalFraction(frac);

  if (!tz) {
    return `${canonicalYear(yy)}-${mo}-${dd}T${hh}:${mi}:${ss}${fraction}`;
  }

  const dt = new Date(0);
  dt.setUTCFullYear(Number(yy), Number(mo) - 1, Number(dd));
  dt.setUTCHours(Number(hh), Number(mi), Number(ss), 0);
  const shifted = new Date(dt.getTime() - tzOffsetMinutes(tz) * 60000);
  const y = canonicalYear(String(shifted.getUTCFullYear()));
  return `${y}-${pad2(shifted.getUTCMonth() + 1)}-${pad2(shifted.getUTCDate())}T`
    + `${pad2(shifted.getUTCHours())}:${pad2(shifted.getUTCMinutes())}:${pad2(shifted.getUTCSeconds())}${fraction}Z`;
}

/** Canonical `xsd:date`: lexical only — canonical year width + tz spelling, NO UTC shift. */
export function canonicalDate(raw: string): string {
  const m = DATE_RE.exec(raw.trim());
  if (!m) throw new Error(`canonicalDate: '${raw}' is not a valid xsd:date lexical`);
  const [, yy, mo, dd, tz] = m;
  return `${canonicalYear(yy)}-${mo}-${dd}${canonicalTz(tz)}`;
}

/** Canonical `xsd:time`: lexical only; `24:00:00` -> `00:00:00`. */
export function canonicalTime(raw: string): string {
  const m = TIME_RE.exec(raw.trim());
  if (!m) throw new Error(`canonicalTime: '${raw}' is not a valid xsd:time lexical`);
  const [, hh, mi, ss, frac, tz] = m;
  const fraction = canonicalFraction(frac);
  let h = hh;
  if (hh === '24' && mi === '00' && ss === '00' && fraction === '') h = '00';
  return `${h}:${mi}:${ss}${fraction}${canonicalTz(tz)}`;
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
    case Carrier.LangString: return canonicalString(raw);
    case Carrier.DateTime: return canonicalDateTime(raw);
    case Carrier.Date: return canonicalDate(raw);
    case Carrier.Time: return canonicalTime(raw);
    case Carrier.HexBinary: return canonicalHexBinary(raw);
    case Carrier.Base64Binary: return canonicalBase64(raw);
  }
}
