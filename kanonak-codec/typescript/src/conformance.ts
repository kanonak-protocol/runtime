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
    const form = packageCanonicalForm(nodes, schema, pkg);
    const hash = packageContentHash(nodes, schema, pkg);
    const ser = nodes.map((n) => serialize(n));

    const formOk = form === c.expectedCanonicalForm;
    const hashOk = hash === c.expectedHash;
    const serOk = JSON.stringify(ser) === JSON.stringify(c.expectedSerialize);

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
        ['serialize', () => nodes.map((n) => serialize(n))],
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
    const ser = nodes.map((n) => serialize(n));
    const roundTripped = nodes.map((n) => deserialize(serialize(n), schema));
    const rtSer = roundTripped.map((n) => serialize(n));
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

if (totalFails > 0) { console.error(`\n${totalFails} FAILURES`); process.exit(1); }
console.log('\nALL VECTORS PASS');
