// The temporal carriers' cases at XSD 1.1's edges, derived rather than hand-typed (runtime#36).
//
// The calendar is ECMAScript's Date: the proleptic Gregorian calendar, with a year 0, as XSD 1.1's is. A year
// outside Date's range is mapped into it through the calendar's 400-year cycle, which repeats exactly. What
// Date doesn't know, XSD 1.1 Part 2 states, and it is declared below: no 60th second, 24:00:00 only as the next
// day's first instant, and a time-zone offset of at most 14:00.
//
// `node generate.mjs` writes ../lexical-temporal-vectors.json; `node generate.mjs --check` fails if it would
// change it.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const OUT = fileURLToPath(new URL('../lexical-temporal-vectors.json', import.meta.url));

const DATE_TIME = /^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;
const DATE = /^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$/;
const TIME = /^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;

const pad = (n, w = 2) => String(n).padStart(w, '0');
const yearText = (y) => (y < 0n ? `-${pad(-y, 4)}` : pad(y, 4));
const fractionText = (f) => {
  const t = (f ?? '').replace(/^\./, '').replace(/0+$/, '');
  return t ? `.${t}` : '';
};

// XSD 1.1's own rules, beyond the calendar.
function offsetMinutes(tz) {
  if (!tz) return undefined;
  if (tz === 'Z') return 0;
  const h = Number(tz.slice(1, 3));
  const m = Number(tz.slice(4, 6));
  if (m > 59 || h > 14 || (h === 14 && m > 0)) return null;
  return (tz[0] === '-' ? -1 : 1) * (h * 60 + m);
}
const tzText = (tz) => (!tz ? '' : tz === 'Z' || tz === '+00:00' || tz === '-00:00' ? 'Z' : tz);
const timeFieldsValid = (h, mi, s, fraction) =>
  mi <= 59 && s <= 59 && (h <= 23 || (h === 24 && mi === 0 && s === 0 && fraction === ''));

// The calendar, from ECMAScript's Date, for a year of any length.
function onCalendar(year, month, day) {
  const cycle = ((year % 400n) + 400n) % 400n;
  const proxy = 2000 + Number(cycle);
  const d = new Date(0);
  d.setUTCFullYear(proxy, month - 1, day);
  if (d.getUTCFullYear() !== proxy || d.getUTCMonth() !== month - 1 || d.getUTCDate() !== day) return null;
  return { proxy, date: d };
}
function shiftDays(year, month, day, days, minutes) {
  const { proxy, date } = onCalendar(year, month, day);
  const t = new Date(date.getTime() + days * 86400000 + minutes * 60000);
  return {
    year: year + BigInt(t.getUTCFullYear() - proxy),
    month: t.getUTCMonth() + 1,
    day: t.getUTCDate(),
    hour: t.getUTCHours(),
    minute: t.getUTCMinutes(),
  };
}

function dateTime(input) {
  const m = DATE_TIME.exec(input);
  if (!m) return null;
  const [, y, mo, d, h, mi, s, f, tz] = m;
  const year = BigInt(y);
  const fraction = fractionText(f);
  const [month, day, hour, minute, second] = [mo, d, h, mi, s].map(Number);
  const offset = offsetMinutes(tz);
  if (offset === null || !onCalendar(year, month, day) || !timeFieldsValid(hour, minute, second, fraction)) return null;
  const at = shiftDays(year, month, day, 0, hour * 60 + minute - (offset ?? 0));
  const clock = `${pad(at.hour)}:${pad(at.minute)}:${pad(second)}${fraction}`;
  return `${yearText(at.year)}-${pad(at.month)}-${pad(at.day)}T${clock}${tz ? 'Z' : ''}`;
}

function date(input) {
  const m = DATE.exec(input);
  if (!m) return null;
  const [, y, mo, d, tz] = m;
  if (offsetMinutes(tz) === null || !onCalendar(BigInt(y), Number(mo), Number(d))) return null;
  return `${yearText(BigInt(y))}-${mo}-${d}${tzText(tz)}`;
}

function time(input) {
  const m = TIME.exec(input);
  if (!m) return null;
  const [, h, mi, s, f, tz] = m;
  const fraction = fractionText(f);
  const [hour, minute, second] = [h, mi, s].map(Number);
  if (offsetMinutes(tz) === null || !timeFieldsValid(hour, minute, second, fraction)) return null;
  return `${hour === 24 ? '00' : h}:${mi}:${s}${fraction}${tzText(tz)}`;
}

// Each input sits at an edge XSD 1.1 draws. Every port must give the same answer at every one of them.
const cases = {
  dateTime: {
    'month-ends-common-year': [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].flatMap((m) => {
      const last = new Date(Date.UTC(2023, m, 0)).getUTCDate();
      return [`2023-${pad(m)}-${pad(last)}T12:00:00`, `2023-${pad(m)}-${pad(last + 1)}T12:00:00`];
    }),
    'leap-years': ['2024-02-29T00:00:00', '2023-02-29T00:00:00', '2000-02-29T00:00:00', '1900-02-29T00:00:00',
      '2100-02-29T00:00:00', '2400-02-29T00:00:00', '0000-02-29T00:00:00', '-0001-02-29T00:00:00',
      '-0004-02-29T00:00:00', '12000-02-29T00:00:00'],
    'field-ranges': ['2026-00-10T00:00:00', '2026-13-10T00:00:00', '2026-05-00T00:00:00',
      '2026-05-31T23:59:59', '2026-05-31T23:60:00', '2026-05-31T25:00:00'],
    'second-sixty': ['2016-12-31T23:59:60Z', '2016-12-31T23:59:60+01:00', '2016-12-31T23:59:60'],
    'hour-twenty-four': ['2026-05-31T24:00:00', '2026-05-31T24:00:00Z', '2026-05-31T24:00:00.000Z',
      '2026-12-31T24:00:00', '9999-12-31T24:00:00', '2026-05-31T24:00:00.5Z', '2026-05-31T24:30:00Z',
      '2026-05-31T24:00:01Z'],
    'offset-bounds': ['2026-05-31T12:00:00+14:00', '2026-05-31T12:00:00-14:00', '2026-05-31T12:00:00+14:01',
      '2026-05-31T12:00:00+15:00', '2026-05-31T12:00:00+05:60', '2026-05-31T12:00:00-00:00'],
    'impossible-day-with-offset': ['2026-02-30T00:00:00+01:00', '2026-04-31T12:00:00Z'],
    'shift-across-boundaries': ['2026-01-01T00:30:00+01:00', '2026-12-31T23:30:00-01:00',
      '2024-02-28T23:00:00-02:00', '2023-02-28T23:00:00-02:00', '2024-03-01T00:30:00+01:00',
      '0001-01-01T00:30:00+01:00', '0000-01-01T00:30:00+01:00', '-0001-12-31T23:30:00-01:00',
      '9999-12-31T23:30:00-01:00', '2026-01-01T00:00:00.120+01:00', '2026-06-15T23:59:59.999-14:00'],
    'years-of-any-length': ['123456789012345678901234567890-12-31T23:30:00-01:00',
      '-123456789012345678901234567890-01-01T00:30:00+01:00', '100000-02-29T12:00:00'],
  },
  date: {
    'calendar': ['2024-02-29', '2023-02-29', '2026-02-30', '2026-04-31', '2026-13-01', '0000-02-29', '2026-02-28'],
    'offset-bounds': ['2026-02-28+14:00', '2026-02-28+15:00', '2026-02-28-00:00'],
  },
  time: {
    'field-ranges': ['23:59:59', '23:60:00', '23:59:60', '25:00:00', '24:00:00', '24:00:00.000', '24:00:01',
      '24:30:00'],
    'offset-bounds': ['12:00:00+14:00', '12:00:00+14:01', '12:00:00-15:00'],
  },
};
const oracle = { dateTime, date, time };

const slug = (s) =>
  s.replace(/^-/, 'minus-').replace(/([+-])(\d\d:\d\d)$/, (_, sign, hm) => (sign === '+' ? 'plus' : 'minus') + hm).replace(/[^0-9A-Za-z]+/g, '_');
const vectors = [];
const ids = new Set();
for (const [carrier, groups] of Object.entries(cases)) {
  for (const [why, inputs] of Object.entries(groups)) {
    for (const input of inputs) {
      const id = `${carrier}-${why}-${slug(input)}`;
      if (ids.has(id)) throw new Error(`two cases are named ${id}: make the input's slug distinct`);
      ids.add(id);
      const expected = oracle[carrier](input);
      vectors.push(expected === null ? { id, carrier, input, expectError: true } : { id, carrier, input, expected });
    }
  }
}

const doc = {
  canonicalFormVersion: '1',
  description:
    'Generated by gen-temporal-vectors/generate.mjs: never edit by hand. The dateTime, date and time carriers at ' +
    "XSD 1.1's edges: impossible days, leap years, a 60th second, 24:00:00, offsets past 14:00, and UTC shifts " +
    'across days, months and years of any length. Same shape as lexical-vectors.json.',
  vectors,
};
const text = JSON.stringify(doc, null, 2) + '\n';

if (process.argv.includes('--check')) {
  let current = '';
  try { current = readFileSync(OUT, 'utf8'); } catch {}
  if (current !== text) {
    console.error(`${OUT} is not what gen-temporal-vectors/generate.mjs makes. Run: node kanonak-canonical/vectors/gen-temporal-vectors/generate.mjs`);
    process.exit(1);
  }
  console.log(`lexical-temporal-vectors.json: ${vectors.length} cases, as generated`);
} else {
  writeFileSync(OUT, text);
  console.log(`wrote ${vectors.length} cases to ${OUT}`);
}
