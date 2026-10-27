/**
 * Generates codec-vectors-json-numbers.json — the JSON forms of integer and
 * decimal values (kanonak-codec 2.2.0, kanonak-protocol/runtime#54), as
 * kanonak-protocol 4.11.0's `numbers-in-json` convention gives them. Every
 * port's `serializeExact` and `deserializeExact`, and each language's JSON
 * binding, read and write exactly these.
 *
 * Unlike the other codec vectors, the expected values are the convention's,
 * stated here case by case, not computed by the reference implementation: what
 * is under test is whether each port follows the convention.
 *
 * Run from the typescript port directory, as the other generators are:
 *   cd kanonak-codec/typescript && npx tsx ../vectors/gen-json-numbers-vectors/gen.ts
 */
import { writeFileSync } from 'node:fs';

const S = 'probe.example.com/schema@1.0.0';
const XSD = 'kanonak.org/core-xsd@1.3.0';
const WIDE = ['integer', 'nonNegativeInteger', 'positiveInteger', 'nonPositiveInteger', 'negativeInteger', 'long', 'unsignedLong'];
const NARROW = ['int', 'short', 'byte', 'unsignedInt', 'unsignedShort', 'unsignedByte'];
const DATATYPES = [...WIDE, 'decimal', ...NARROW];

const prop = (name: string, datatype: string) => ({ predicate: `${S}/${name}`, kind: 'datatype', datatype: `${XSD}/${datatype}` });

const schema = {
  typePredicate: 'kanonak.org/core-rdf@1.2.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.2.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@2.4.0/Package',
  classes: {
    [`${S}/Numbers`]: {
      typeUri: `${S}/Numbers`,
      props: {
        ...Object.fromEntries(DATATYPES.map((d) => [d, prop(d, d)])),
        counts: prop('counts', 'integer'),
        amount: { predicate: `${S}/amount`, kind: 'object', range: `${S}/Amount` },
      },
    },
    [`${S}/Amount`]: { typeUri: `${S}/Amount`, props: { value: prop('value', 'decimal') } },
  },
};

type Lexical = string | string[] | Record<string, string>;
const readCases: object[] = [];
const writeCases: object[] = [];
const read = (id: string, field: string, json: string, expected?: Lexical) =>
  readCases.push(expected === undefined ? { id, field, json, expectError: true } : { id, field, json, expected });
const write = (id: string, field: string, value: Lexical, json?: string) =>
  writeCases.push(json === undefined ? { id, field, value, expectError: true } : { id, field, value, json });

// integer: a string holding any lexical form of the datatype, or a whole JSON number within ±(2^53 − 1)
read('integer-string-past-2^53', 'integer', '"9007199254740993"', '9007199254740993');
read('integer-string-thirty-digits', 'integer', '"-123456789012345678901234567890"', '-123456789012345678901234567890');
read('integer-string-not-canonical', 'integer', '"+007"', '7');
read('integer-number', 'integer', '42', '42');
read('integer-number-max-safe', 'integer', '9007199254740991', '9007199254740991');
read('integer-number-min-safe', 'integer', '-9007199254740991', '-9007199254740991');
read('integer-number-whole-with-fraction-token', 'integer', '2.0', '2');
read('integer-number-exponent', 'integer', '1e3', '1000');
read('integer-number-2^53', 'integer', '9007199254740992');
read('integer-number-past-2^53', 'integer', '9007199254740993');
read('integer-number-minus-2^53', 'integer', '-9007199254740992');
read('integer-number-fraction', 'integer', '1.5');
read('integer-string-not-a-lexical', 'integer', '"12x"');
read('integer-string-fraction', 'integer', '"1.5"');
read('integer-string-empty', 'integer', '""');
read('integer-boolean', 'integer', 'true');
// the four sign-restricted forms read as integer does, inside their range
read('nonNegativeInteger-zero', 'nonNegativeInteger', '"0"', '0');
read('nonNegativeInteger-negative', 'nonNegativeInteger', '"-1"');
read('positiveInteger-one', 'positiveInteger', '1', '1');
read('positiveInteger-zero', 'positiveInteger', '"0"');
read('nonPositiveInteger-zero', 'nonPositiveInteger', '"0"', '0');
read('nonPositiveInteger-one', 'nonPositiveInteger', '"1"');
read('negativeInteger-minus-one', 'negativeInteger', '"-1"', '-1');
read('negativeInteger-zero', 'negativeInteger', '0');
// long and unsignedLong: as integer, and inside their width
read('long-max', 'long', '"9223372036854775807"', '9223372036854775807');
read('long-min', 'long', '"-9223372036854775808"', '-9223372036854775808');
read('long-number', 'long', '5', '5');
read('long-past-max', 'long', '"9223372036854775808"');
read('long-past-min', 'long', '"-9223372036854775809"');
read('unsignedLong-max', 'unsignedLong', '"18446744073709551615"', '18446744073709551615');
read('unsignedLong-number', 'unsignedLong', '7', '7');
read('unsignedLong-past-max', 'unsignedLong', '"18446744073709551616"');
read('unsignedLong-negative', 'unsignedLong', '"-1"');
// decimal: a string only
read('decimal-string', 'decimal', '"-7.001"', '-7.001');
read('decimal-string-trailing-zero', 'decimal', '"1.50"', '1.5');
read('decimal-string-thirty-digits', 'decimal', '"12345678901234567890.1234567890"', '12345678901234567890.123456789');
read('decimal-string-whole', 'decimal', '"5"', '5');
read('decimal-number', 'decimal', '1.5');
read('decimal-number-whole', 'decimal', '2');
read('decimal-string-exponent', 'decimal', '"1e3"');
read('decimal-string-not-a-lexical', 'decimal', '"abc"');
// int and narrower: a whole JSON number only, inside the datatype's range
read('int-max', 'int', '2147483647', '2147483647');
read('int-min', 'int', '-2147483648', '-2147483648');
read('int-past-max', 'int', '2147483648');
read('int-string', 'int', '"5"');
read('int-fraction', 'int', '1.5');
read('short-min', 'short', '-32768', '-32768');
read('short-past-max', 'short', '32768');
read('byte-max', 'byte', '127', '127');
read('byte-past-max', 'byte', '128');
read('unsignedInt-max', 'unsignedInt', '4294967295', '4294967295');
read('unsignedInt-negative', 'unsignedInt', '-1');
read('unsignedShort-past-max', 'unsignedShort', '65536');
read('unsignedByte-max', 'unsignedByte', '255', '255');
read('unsignedByte-past-max', 'unsignedByte', '256');
// each item of a list, and each field of an embedded value, reads by its own datatype
read('list-of-integer', 'counts', '["9007199254740993", 5]', ['9007199254740993', '5']);
read('list-of-integer-with-unsafe-number', 'counts', '[1, 9007199254740993]');
read('embedded-decimal', 'amount', '{"value": "1.50"}', { value: '1.5' });
read('embedded-decimal-number', 'amount', '{"value": 1.5}');

// writing: a wide integer and a decimal as a string of its canonical lexical form, a narrow integer as a number
write('integer-past-2^53', 'integer', '9007199254740993', '"9007199254740993"');
write('integer-small', 'integer', '42', '"42"');
write('integer-thirty-digits', 'integer', '-123456789012345678901234567890', '"-123456789012345678901234567890"');
write('nonNegativeInteger-negative', 'nonNegativeInteger', '-1');
write('positiveInteger-zero', 'positiveInteger', '0');
write('long-min', 'long', '-9223372036854775808', '"-9223372036854775808"');
write('long-past-max', 'long', '9223372036854775808');
write('unsignedLong-max', 'unsignedLong', '18446744073709551615', '"18446744073709551615"');
write('unsignedLong-negative', 'unsignedLong', '-1');
write('decimal', 'decimal', '-7.001', '"-7.001"');
write('decimal-thirty-digits', 'decimal', '12345678901234567890.123456789', '"12345678901234567890.123456789"');
write('int', 'int', '42', '42');
write('int-past-max', 'int', '2147483648');
write('byte-min', 'byte', '-128', '-128');
write('unsignedShort-past-max', 'unsignedShort', '65536');
write('list-of-integer', 'counts', ['1', '9007199254740993'], '["1", "9007199254740993"]');
write('embedded-decimal', 'amount', { value: '1.5' }, '{"value": "1.5"}');

const out = {
  description:
    'The JSON forms of integer and decimal values (kanonak-codec 2.2.0, runtime#54), as kanonak-protocol 4.11.0\'s ' +
    'numbers-in-json convention gives them. Every port\'s serializeExact and deserializeExact, and each language\'s ' +
    'JSON binding, read and write exactly these. readCases: a node of `class` whose `field` holds the JSON text `json` ' +
    'reads as `expected`, the canonical lexical form of each value (a list item by item, an embedded value field by ' +
    'field, each by its own datatype), or, with expectError, is refused. A wide integer (integer, its four ' +
    'sign-restricted forms, long, unsignedLong) reads from a string holding a lexical form of its datatype, or from a ' +
    'JSON number whose value is a whole number within ±(2^53 − 1); decimal from a string only; int and narrower from ' +
    'a whole JSON number only; every value inside its datatype\'s range. writeCases: the value whose canonical ' +
    'lexical form is `value` writes as the JSON text `json`, compared as parsed JSON, or, with expectError, is ' +
    'refused for lying outside its datatype\'s range. The expected values are the convention\'s, stated case by case ' +
    'in gen-json-numbers-vectors/gen.ts.',
  canonicalFormVersion: '1',
  datatypes: Object.fromEntries(DATATYPES.map((d) => [d, `${XSD}/${d}`])),
  class: `${S}/Numbers`,
  schema,
  readCases,
  writeCases,
};

const target = new URL('../codec-vectors-json-numbers.json', import.meta.url);
writeFileSync(target, JSON.stringify(out, null, 1) + '\n', 'utf8');
console.log(`wrote ${target.pathname} — ${readCases.length} read cases, ${writeCases.length} write cases`);
