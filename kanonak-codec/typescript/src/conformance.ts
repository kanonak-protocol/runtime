/**
 * Drives the shared codec vectors through this port: typed nodes + the embedded
 * schema -> canonical form + content hash + normalized-JSON serialize. Runs the
 * 0.1.0 contract file (codec-vectors.json), the 0.2.0 embedded-values file
 * (codec-vectors-embedded.json), and the 0.4.0 multi-typed-subjects file
 * (codec-vectors-types.json, runtime#10). Run: `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import {
  deserialize,
  typeMatches,
  enumMember,
  embed,
  packageCanonicalForm,
  packageContentHash,
  ref,
  refTo,
  serialize,
  toNode,
  type CodecSchema,
  type CodecNode,
  type KanonakNode,
  type PackageContext,
  type Ref,
} from './index.js';

let totalFails = 0;

function runFile(relative: string): void {
  const vfile = new URL(`../../vectors/${relative}`, import.meta.url);
  const d: any = JSON.parse(readFileSync(vfile, 'utf8'));
  const schema: CodecSchema = d.schema;

  let fails = 0;
  let pass = 0;
  for (const c of d.cases) {
    const nodes: CodecNode[] = c.nodes;
    const pkg: PackageContext = c.pkg;
    const ser = nodes.map((n) => serialize(n, schema));
    const serOk = JSON.stringify(ser) === JSON.stringify(c.expectedSerialize);

    if (c.expectHashError) {
      // Serializing still carries what hashing refuses (runtime#37).
      const refuses = (what: string, run: () => unknown): boolean => {
        try {
          run();
          console.error(`${c.id}: expected ${what} to refuse with [${c.expectHashError}], it did not`);
          return false;
        } catch (err) {
          const ok = (err as Error).message.endsWith(`[${c.expectHashError}]`);
          if (!ok) console.error(`${c.id}: ${what} expected [${c.expectHashError}], got: ${(err as Error).message}`);
          return ok;
        }
      };
      const formOk = refuses('the canonical form', () => packageCanonicalForm(nodes, schema, pkg));
      const hashOk = refuses('the hash', () => packageContentHash(nodes, schema, pkg));
      if (!serOk) console.error(`${c.id}: serialize mismatch\n  expected ${JSON.stringify(c.expectedSerialize)}\n  got      ${JSON.stringify(ser)}`);
      formOk && hashOk && serOk ? pass++ : fails++;
      continue;
    }

    const form = packageCanonicalForm(nodes, schema, pkg);
    const hash = packageContentHash(nodes, schema, pkg);
    const formOk = form === c.expectedCanonicalForm;
    const hashOk = hash === c.expectedHash;

    if (formOk && hashOk && serOk) {
      pass++;
    } else {
      fails++;
      if (!formOk) console.error(`${c.id}: canonical form mismatch\n  expected ${c.expectedCanonicalForm}\n  got      ${form}`);
      if (!hashOk) console.error(`${c.id}: hash expected ${c.expectedHash} got ${hash}`);
      if (!serOk) console.error(`${c.id}: serialize mismatch\n  expected ${JSON.stringify(c.expectedSerialize)}\n  got      ${JSON.stringify(ser)}`);
    }
  }
  console.log(`${relative}: ${pass}/${d.cases.length} pass`);
  totalFails += fails;
}

/**
 * The 0.4.0 multi-typed-subjects file (runtime#10). Beyond the standard
 * form/hash/serialize checks it exercises the $types contract:
 *  - expectError cases must be rejected on ALL THREE surfaces — serialize (the
 *    producer fails at emit time), deserialize (the reader rejects, never
 *    repairs), and canonicalization;
 *  - positive cases must round-trip: deserialize(serialize(x)) preserves
 *    $types exactly and re-canonicalizes to the same hash.
 */
/**
 * The 0.5.0 enumerations file (`enums`, runtime#21). Beyond the standard
 * form/hash/serialize checks it pins the enumeration contract:
 *
 *  - the schema PARSES with `enums` present, keyed by durable VERSIONED URIs at
 *    both levels — a versionless key would look right and miss every lookup, so
 *    the formation is asserted, not assumed;
 *  - an enumeration STANDS ALONE: its class has no `classes` twin;
 *  - `enums` and an enum-ranged `range` are canonicalization-INERT (the
 *    positive hashes are the ones the same nodes produce without them);
 *  - `expectError` cases must be rejected at CANONICALIZATION. Unlike the
 *    $types file's all-three-surfaces contract, this violation is
 *    schema-DEPENDENT: `serialize` and `deserialize` carry an embedded value
 *    whose class the schema lacks as it stands, so canonicalization is the
 *    only surface that can see it. Requiring the other two would be requiring them to do
 *    something they are deliberately not built to do.
 */
function runEnumsFile(relative: string): void {
  const vfile = new URL(`../../vectors/${relative}`, import.meta.url);
  const d: any = JSON.parse(readFileSync(vfile, 'utf8'));
  const schema: CodecSchema = d.schema;

  let fails = 0;
  let pass = 0;

  // -- structural assertions on the schema itself -------------------------
  const enums = schema.enums;
  if (!enums || Object.keys(enums).length === 0) {
    console.error(`${relative}: schema carries no enums`);
    fails++;
  } else {
    for (const [key, en] of Object.entries(enums)) {
      if (key !== en.typeUri) {
        console.error(`${relative}: enum key ${key} != typeUri ${en.typeUri}`);
        fails++;
      }
      // An enumeration stands alone: no `classes` twin is required, and the
      // absence is load-bearing — it is what makes an embedded value on an
      // enum-ranged property fail instead of being mapped as a resource.
      if (schema.classes[key]) {
        console.error(`${relative}: enum ${key} must NOT also appear in classes`);
        fails++;
      }
      for (const memberUri of Object.keys(en.members)) {
        // Durable VERSIONED formation, byte-identical to the wire `$ref`.
        if (!/^[^/]+\/[^/@]+@\d+\.\d+\.\d+\/[^/]+$/.test(memberUri)) {
          console.error(`${relative}: member key ${memberUri} is not a versioned durable URI`);
          fails++;
        }
      }
    }
    if (fails === 0) pass++;
  }

  // -- cases ---------------------------------------------------------------
  for (const c of d.cases) {
    const nodes: CodecNode[] = c.nodes;
    const pkg: PackageContext = c.pkg;

    if (c.expectError) {
      let threw = false;
      try { packageCanonicalForm(nodes, schema, pkg); } catch { threw = true; }
      if (threw) pass++;
      else { fails++; console.error(`${c.id}: expected canonicalization to reject, it did not`); }
      continue;
    }

    const form = packageCanonicalForm(nodes, schema, pkg);
    const hash = packageContentHash(nodes, schema, pkg);
    const ser = nodes.map((n) => serialize(n, schema));

    const formOk = form === c.expectedCanonicalForm;
    const hashOk = hash === c.expectedHash;
    const serOk = JSON.stringify(ser) === JSON.stringify(c.expectedSerialize);

    if (formOk && hashOk && serOk) pass++;
    else {
      fails++;
      if (!formOk) console.error(`${c.id}: canonical form mismatch\n  expected ${c.expectedCanonicalForm}\n  got      ${form}`);
      if (!hashOk) console.error(`${c.id}: hash expected ${c.expectedHash} got ${hash}`);
      if (!serOk) console.error(`${c.id}: serialize mismatch`);
    }
  }

  console.log(`${relative}: ${pass}/${d.cases.length + 1} pass (cases + schema shape)`);
  totalFails += fails;
}

function runTypesFile(relative: string): void {
  const vfile = new URL(`../../vectors/${relative}`, import.meta.url);
  const d: any = JSON.parse(readFileSync(vfile, 'utf8'));
  const schema: CodecSchema = d.schema;

  let fails = 0;
  let pass = 0;
  for (const c of d.cases) {
    const nodes: CodecNode[] = c.nodes;
    const pkg: PackageContext = c.pkg;

    if (c.expectError) {
      const surfaces: Array<[string, () => unknown]> = [
        ['canonicalize', () => packageCanonicalForm(nodes, schema, pkg)],
        ['serialize', () => nodes.map((n) => serialize(n, schema))],
        ['deserialize', () => nodes.map((n) => deserialize(n as Record<string, unknown>, schema))],
      ];
      let ok = true;
      for (const [what, run] of surfaces) {
        let threw = false;
        try { run(); } catch { threw = true; }
        if (!threw) {
          ok = false;
          console.error(`${c.id}: expected ${what} to reject, it did not`);
        }
      }
      ok ? pass++ : fails++;
      continue;
    }

    const form = packageCanonicalForm(nodes, schema, pkg);
    const hash = packageContentHash(nodes, schema, pkg);
    const ser = nodes.map((n) => serialize(n, schema));
    const roundTripped = nodes.map((n) => deserialize(serialize(n, schema), schema));
    const rtSer = roundTripped.map((n) => serialize(n, schema));
    const rtHash = packageContentHash(roundTripped, schema, pkg);

    const formOk = form === c.expectedCanonicalForm;
    const hashOk = hash === c.expectedHash;
    const serOk = JSON.stringify(ser) === JSON.stringify(c.expectedSerialize);
    const rtOk = JSON.stringify(rtSer) === JSON.stringify(c.expectedSerialize) && rtHash === c.expectedHash;

    if (formOk && hashOk && serOk && rtOk) {
      pass++;
    } else {
      fails++;
      if (!formOk) console.error(`${c.id}: canonical form mismatch\n  expected ${c.expectedCanonicalForm}\n  got      ${form}`);
      if (!hashOk) console.error(`${c.id}: hash expected ${c.expectedHash} got ${hash}`);
      if (!serOk) console.error(`${c.id}: serialize mismatch\n  expected ${JSON.stringify(c.expectedSerialize)}\n  got      ${JSON.stringify(ser)}`);
      if (!rtOk) console.error(`${c.id}: round-trip mismatch\n  expected ${JSON.stringify(c.expectedSerialize)} @ ${c.expectedHash}\n  got      ${JSON.stringify(rtSer)} @ ${rtHash}`);
    }
  }
  console.log(`${relative}: ${pass}/${d.cases.length} pass`);
  totalFails += fails;
}

runFile('codec-vectors.json');
runFile('codec-vectors-embedded.json');
runTypesFile('codec-vectors-types.json');
runTypesFile('codec-vectors-references.json');
runEnumsFile('codec-vectors-enums.json');
runCompatFile('codec-vectors-compat.json');

/**
 * The compatibility file (0.6.1, runtime#28): deserialize / typeMatches /
 * enumMember / hashing over a node typed at an earlier compatible version of
 * the schema's package. A rejection must end in the bracketed kind the vector
 * names — the one part of an error message every port reproduces.
 */
function runCompatFile(relative: string): void {
  const vfile = new URL(`../../vectors/${relative}`, import.meta.url);
  const d: any = JSON.parse(readFileSync(vfile, 'utf8'));
  let pass = 0;
  let total = 0;
  const rejected = (fn: () => unknown, kind: string, id: string): boolean => {
    try {
      fn();
      console.error(`${id}: expected a [${kind}] rejection, got a value`);
      return false;
    } catch (err) {
      const ok = (err as Error).message.endsWith(`[${kind}]`);
      if (!ok) console.error(`${id}: expected [${kind}], got: ${(err as Error).message}`);
      return ok;
    }
  };
  for (const c of d.deserializeCases) {
    total++;
    const schema = d.schemas[c.schema];
    let ok: boolean;
    if (c.expectError) ok = rejected(() => deserialize(c.input, schema), c.expectError, c.id);
    else {
      const got = deserialize(c.input, schema);
      ok = JSON.stringify(got) === JSON.stringify(c.expected);
      if (!ok) console.error(`${c.id}: expected ${JSON.stringify(c.expected)} got ${JSON.stringify(got)}`);
    }
    if (ok) pass++; else totalFails++;
  }
  for (const c of d.typeMatchesCases) {
    total++;
    const got = typeMatches(c.node, c.classUri, d.schemas[c.schema]);
    if (got === c.expected) pass++;
    else { totalFails++; console.error(`${c.id}: typeMatches expected ${c.expected} got ${got}`); }
  }
  for (const c of d.enumMemberCases) {
    total++;
    const m = enumMember(d.schemas[c.schema], c.ref);
    const got = m ? { enumType: m.enumType, uri: m.uri, label: m.member.label ?? null } : null;
    if (JSON.stringify(got) === JSON.stringify(c.expected)) pass++;
    else { totalFails++; console.error(`${c.id}: enumMember expected ${JSON.stringify(c.expected)} got ${JSON.stringify(got)}`); }
  }
  for (const c of d.hashCases) {
    total++;
    const schema = d.schemas[c.schema];
    let ok: boolean;
    if (c.expectError) ok = rejected(() => packageContentHash(c.nodes, schema, d.pkg), c.expectError, c.id);
    else {
      const got = packageContentHash(c.nodes, schema, d.pkg);
      ok = got === c.expectedHash;
      if (!ok) console.error(`${c.id}: hash expected ${c.expectedHash} got ${got}`);
    }
    if (ok) pass++; else totalFails++;
  }
  console.log(`${relative}: ${pass}/${total} pass`);
}

// -- Typed-surface conformance: generated-style typed objects (KanonakNode +
//    Ref<T> arm constructors) reproduce the SAME golden vectors. Also the
//    executable spec for the TS SDK generator's target shape.

interface Order extends KanonakNode {
  note?: string;
  items?: Ref<LineItem>[];
  customer?: Ref<Customer>[] | Ref<Customer>; // list in most cases; bare in single-embedded-bare
}
interface LineItem extends KanonakNode { sku?: string; qty?: number }
interface Customer extends KanonakNode { name?: string; address?: Ref<Address>[] }
interface Address extends KanonakNode { city?: string }
interface Person extends KanonakNode { name?: string }
interface Account extends KanonakNode {
  accountCode?: string; seats?: number; rate?: number; active?: boolean;
  owner?: Ref<Person>; tags?: string[];
}

const SCHEMA_NS = 'probe.example.com/schema@1.0.0';
const DATA = 'probe.example.com/data@1.0.0';

function loadDoc(relative: string): any {
  return JSON.parse(readFileSync(new URL(`../../vectors/${relative}`, import.meta.url), 'utf8'));
}

function checkTyped(doc: any, caseId: string, typed: KanonakNode[]): void {
  typedTotal++;
  const c = doc.cases.find((x: any) => x.id === caseId);
  if (!c) { totalFails++; console.error(`typed ${caseId}: vector case not found`); return; }
  const schema: CodecSchema = doc.schema;
  const nodes = typed.map((t) => toNode(t, schema));
  const form = packageCanonicalForm(nodes, schema, c.pkg);
  const hash = packageContentHash(nodes, schema, c.pkg);
  if (form !== c.expectedCanonicalForm) {
    totalFails++;
    console.error(`typed ${caseId}: canonical form mismatch\n  expected ${c.expectedCanonicalForm}\n  got      ${form}`);
  } else if (hash !== c.expectedHash) {
    totalFails++;
    console.error(`typed ${caseId}: hash expected ${c.expectedHash} got ${hash}`);
  } else {
    typedPass++;
  }
}

let typedPass = 0;
let typedTotal = 0;
const emb = loadDoc('codec-vectors-embedded.json');
const bas = loadDoc('codec-vectors.json');
const typ = loadDoc('codec-vectors-types.json');

const namedInList: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`, note: 'A',
  items: [embed<LineItem>({ sku: 'X', qty: 1 }, 'first')],
};
checkTyped(emb, 'embedded-named-in-list', [namedInList]);

const unnamedPositional: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`, note: 'A',
  items: [embed<LineItem>({ sku: 'X', qty: 1 })],
};
checkTyped(emb, 'embedded-unnamed-positional', [unnamedPositional]);

const explicitType: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`, note: 'A',
  items: [embed<LineItem>({ $type: `${SCHEMA_NS}/LineItem`, sku: 'X', qty: 1 }, 'first')],
};
checkTyped(emb, 'embedded-explicit-type', [explicitType]);

const listOrder: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`,
  items: [
    embed<LineItem>({ sku: 'X', qty: 1 }, 'a'),
    embed<LineItem>({ sku: 'Y', qty: 2 }, 'b'),
  ],
};
checkTyped(emb, 'embedded-list-order', [listOrder]);

const nested: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`,
  customer: [embed<Customer>({
    name: 'Ada',
    address: [embed<Address>({ city: 'Austin' }, 'home')],
  }, 'cust')],
};
checkTyped(emb, 'embedded-nested', [nested]);

const bareCustomer: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`,
  customer: embed<Customer>({ name: 'Ada' }, 'cust'),
};
checkTyped(emb, 'single-embedded-bare', [bareCustomer]);

const emptyList: Order = {
  $id: `${DATA}/o1`, $type: `${SCHEMA_NS}/Order`, note: 'A', items: [],
};
checkTyped(emb, 'empty-list-emits-nothing', [emptyList]);

const alice: Person = { $id: `${DATA}/p1`, $type: `${SCHEMA_NS}/Person`, name: 'Alice' };
for (const owner of [ref(`${DATA}/p1`), refTo(alice)]) {
  const account: Account = {
    $id: `${DATA}/a1`, $type: `${SCHEMA_NS}/Account`,
    accountCode: 'paul', seats: 5, rate: 1.5, active: true,
    owner, tags: ['x', 'y'],
  };
  checkTyped(bas, 'basic-scalars-ref-list', [alice, account]);
}

// -- Typed-surface $types cases (0.4.0, runtime#10): the multi-typed set rides
//    the generated model as $types only (no unprefixed accessor) and reproduces
//    the same golden vectors through toNode.

interface DefResource extends KanonakNode { note?: string }
interface Bundle extends KanonakNode { parts?: Ref<PartDef>[] | Ref<PartDef> }
interface PartDef extends KanonakNode { size?: number }

const coveredSet: DefResource = {
  $id: `${DATA}/w1`, $type: `${SCHEMA_NS}/ClassDef`,
  $types: [`${SCHEMA_NS}/AnnotatedDef`, `${SCHEMA_NS}/ClassDef`],
  note: 'A',
};
checkTyped(typ, 'covered-redundant-set', [coveredSet]);

const multiTypedEmbedded: Bundle = {
  $id: `${DATA}/b1`, $type: `${SCHEMA_NS}/Bundle`,
  parts: embed<PartDef>({
    $type: `${SCHEMA_NS}/PartDef`,
    $types: [`${SCHEMA_NS}/PartDef`, `${SCHEMA_NS}/SealedDef`],
    size: 2,
  }, 'first'),
};
checkTyped(typ, 'embedded-multi-typed-named', [multiTypedEmbedded]);

const mixedListItems: Bundle = {
  $id: `${DATA}/b1`, $type: `${SCHEMA_NS}/Bundle`,
  parts: [
    embed<PartDef>({
      $type: `${SCHEMA_NS}/PartDef`,
      $types: [`${SCHEMA_NS}/PartDef`, `${SCHEMA_NS}/SealedDef`],
      size: 1,
    }, 'a'),
    embed<PartDef>({ $type: `${SCHEMA_NS}/PartDef`, size: 2 }, 'b'),
  ],
};
checkTyped(typ, 'types-in-list-items', [mixedListItems]);

console.log(`typed surface: ${typedPass}/${typedTotal} pass`);

// TypeScript only, since every other port keeps a number's exact source text: a JavaScript number holds every integer
// exactly only up to 2^53, so an integer or decimal past it is refused, and a bigint carries it exactly (runtime#37).
{
  const d: any = JSON.parse(readFileSync(new URL('../../vectors/codec-vectors.json', import.meta.url), 'utf8'));
  const schema: CodecSchema = d.schema;
  const pkg: PackageContext = d.cases[0].pkg;
  const account = (fields: Record<string, unknown>): CodecNode =>
    ({ $id: 'probe.example.com/data@1.0.0/big', $type: 'probe.example.com/schema@1.0.0/Account', ...fields }) as CodecNode;
  let pass = 0;
  const expectRefused = (label: string, node: CodecNode): void => {
    try {
      packageContentHash([node], schema, pkg);
      totalFails++; console.error(`unsafe-number ${label}: expected a refusal, got a hash`);
    } catch (e) {
      if ((e as Error).message.endsWith('[unsafe-number]')) pass++;
      else { totalFails++; console.error(`unsafe-number ${label}: refused for another reason: ${(e as Error).message}`); }
    }
  };
  expectRefused('integer number past 2^53', account({ seats: 9007199254740993 }));
  expectRefused('decimal number past 2^53', account({ rate: 12345678901234567890 }));
  const fromBigint = packageContentHash([account({ seats: BigInt('9007199254740993') })], schema, pkg);
  const fromString = packageContentHash([account({ seats: '9007199254740993' })], schema, pkg);
  if (fromBigint === fromString) pass++;
  else { totalFails++; console.error(`unsafe-number bigint: ${fromBigint} differs from the string form's ${fromString}`); }
  const safe = packageContentHash([account({ seats: 42 })], schema, pkg);
  if (safe === packageContentHash([account({ seats: '42' })], schema, pkg)) pass++;
  else { totalFails++; console.error('unsafe-number safe integer: a number and its string hash differently'); }
  console.log(`unsafe numbers (TypeScript only): ${pass}/4 pass`);
}

// The JSON forms of integer and decimal values (runtime#54): serialize and deserialize against the shared vectors.
{
  const relative = 'codec-vectors-json-numbers.json';
  const d: any = JSON.parse(readFileSync(new URL(`../../vectors/${relative}`, import.meta.url), 'utf8'));
  const schema: CodecSchema = d.schema;
  const decimalFields = new Set(['decimal']);
  const narrowFields = new Set(['int', 'short', 'byte', 'unsignedInt', 'unsignedShort', 'unsignedByte']);
  // This port's exact type for a value, from its canonical lexical form: a bigint, a decimal's digits, or a number.
  const exact = (field: string, lexical: unknown): unknown => {
    if (Array.isArray(lexical)) return lexical.map((item) => exact(field, item));
    const text = lexical as string;
    return decimalFields.has(field) ? text : narrowFields.has(field) ? Number(text) : BigInt(text);
  };
  // A value read back as its canonical lexical form, item by item and field by field.
  const lexicalOf = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(lexicalOf);
    if (value && typeof value === 'object') {
      return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([k, v]) => [k, lexicalOf(v)]));
    }
    return String(value);
  };
  let pass = 0;
  for (const c of d.readCases) {
    const json = JSON.parse(`{"$type": ${JSON.stringify(d.class)}, ${JSON.stringify(c.field)}: ${c.json}}`);
    let got: unknown;
    try {
      got = lexicalOf((deserialize(json, schema) as Record<string, unknown>)[c.field]);
    } catch (e) {
      if (c.expectError && (e as Error).message.endsWith('[json-number-form]')) pass++;
      else { totalFails++; console.error(`${relative} read ${c.id}: threw ${(e as Error).message}`); }
      continue;
    }
    if (c.expectError) { totalFails++; console.error(`${relative} read ${c.id}: expected a refusal, got ${JSON.stringify(got)}`); continue; }
    if (JSON.stringify(got) === JSON.stringify(c.expected)) pass++;
    else { totalFails++; console.error(`${relative} read ${c.id}: expected ${JSON.stringify(c.expected)}, got ${JSON.stringify(got)}`); }
  }
  for (const c of d.writeCases) {
    // An embedded Amount's one field is a decimal, which this port holds as its digits.
    const value = c.field === 'amount' ? { value: c.value.value } : exact(c.field, c.value);
    let got: unknown;
    try {
      got = serialize({ $type: d.class, [c.field]: value } as CodecNode, schema)[c.field];
    } catch (e) {
      if (c.expectError && (e as Error).message.endsWith('[json-number-form]')) pass++;
      else { totalFails++; console.error(`${relative} write ${c.id}: threw ${(e as Error).message}`); }
      continue;
    }
    if (c.expectError) { totalFails++; console.error(`${relative} write ${c.id}: expected a refusal, got ${JSON.stringify(got)}`); continue; }
    if (JSON.stringify(got) === JSON.stringify(JSON.parse(c.json))) pass++;
    else { totalFails++; console.error(`${relative} write ${c.id}: expected ${c.json}, got ${JSON.stringify(got)}`); }
  }
  console.log(`${relative}: ${pass}/${d.readCases.length + d.writeCases.length} pass`);
}

if (totalFails > 0) { console.error(`\n${totalFails} FAILURES`); process.exit(1); }
console.log('\nALL VECTORS PASS');
