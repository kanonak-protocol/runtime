/**
 * Generates codec-vectors-enums.json — the conformance vectors for ENUMERATIONS
 * in the schema (`enums`, kanonak-codec 0.5.0, kanonak-protocol/runtime#21) —
 * from the TypeScript reference implementation. Expected values are
 * authoritative once committed; regeneration must be byte-identical unless the
 * contract itself changes.
 *
 * What this file pins, and why each case exists:
 *
 *  1. `enums` is CARRIED and canonicalization-INERT. The positive case hashes
 *     a package whose schema declares an enumeration; the hash must equal what
 *     the same nodes produce with no `enums` at all. Inertness measured, not
 *     assumed.
 *
 *  2. `range` on an enum-ranged property is inert TOO. The property below
 *     declares `range` at the enumeration class so a consumer can get from a
 *     property to its enumeration without scanning every entry. `range` is
 *     read ONLY inside the embedded path, and a `{"$ref": …}` returns before
 *     reaching it — so the inertness is structural, not incidental.
 *
 *  3. That structural guarantee holds only while an enum member is never
 *     embeddable, so the negative case PINS it: an embedded value on an
 *     enum-ranged property must be REJECTED. Without this case the guarantee
 *     would live in a comment, and a comment cannot fail — a later change
 *     making members embeddable would surface as a diverging content hash,
 *     which is the most expensive way to learn it.
 *
 * ERROR SURFACE. Unlike codec-vectors-types.json — whose `$types` violations
 * are schema-free and so must be rejected on all three surfaces — this file's
 * violation is schema-DEPENDENT. `serialize` is schema-free and `deserialize`
 * does not recurse into embedded values, so CANONICALIZATION is the one
 * surface that can see it, and the only one required to reject.
 *
 * Run from the typescript port directory (so module resolution finds
 * @kanonak-protocol/canonical):
 *
 *   cd kanonak-codec/typescript && npx tsx ../vectors/gen-enums-vectors/gen.ts
 */
import { writeFileSync } from 'node:fs';
import {
  packageCanonicalForm,
  packageContentHash,
  serialize,
  type CodecNode,
  type CodecSchema,
  type PackageContext,
} from '../../typescript/src/index.js';

const NS = 'probe.example.com/schema@1.0.0';
const D = 'probe.example.com/data@1.0.0';

/**
 * `Condition` is declared ONLY under `enums` — it has deliberately NO `classes`
 * entry. That is the 0.5.0 position: a `classes` entry is required only when
 * instances of the class are emitted as nodes, which an enumeration's members
 * never are. The absence is what makes the negative case below fail loudly.
 */
const schema: CodecSchema = {
  typePredicate: 'kanonak.org/core-rdf@1.1.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.1.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@1.0.0/Package',
  classes: {
    [`${NS}/Product`]: {
      typeUri: `${NS}/Product`,
      props: {
        sku: {
          predicate: `${NS}/sku`,
          kind: 'datatype',
          datatype: 'kanonak.org/core-xsd/string',
        },
        // The enum-ranged property: `range` points at the enumeration class so
        // a consumer can go property -> enumeration directly instead of
        // scanning `enums`. Its value is always a reference, never embedded.
        condition: {
          predicate: `${NS}/condition`,
          kind: 'object',
          range: `${NS}/Condition`,
        },
      },
    },
  },
  enums: {
    [`${NS}/Condition`]: {
      typeUri: `${NS}/Condition`,
      // Keyed by durable VERSIONED URI — byte-identical to the wire `$ref`.
      members: {
        [`${NS}/condition-new`]: { label: 'New' },
        [`${NS}/condition-used`]: { label: 'Used' },
      },
    },
  },
};

/** The same schema with `enums` removed — the inertness control. */
const schemaWithoutEnums: CodecSchema = {
  typePredicate: schema.typePredicate,
  labelPredicate: schema.labelPredicate,
  packageTypeUri: schema.packageTypeUri,
  classes: schema.classes,
};

const pkg: PackageContext = {
  publisher: 'probe.example.com',
  packageName: 'data',
  version: '1.0.0',
  label: 'Enums Probe Data',
};

interface Case {
  id: string;
  description: string;
  pkg: PackageContext;
  nodes: CodecNode[];
  expectError?: true;
  expectedCanonicalForm?: string;
  expectedHash?: string;
  expectedSerialize?: unknown[];
}

const memberRefNodes: CodecNode[] = [
  {
    $type: `${NS}/Product`,
    $id: `${D}/prod-1`,
    sku: 'SKU-1',
    condition: { $ref: `${NS}/condition-new` },
  } as CodecNode,
];

const cases: Case[] = [
  {
    id: 'enum-member-ref-hash-inert',
    description:
      'A member URI crosses the wire as {"$ref": …} on a property ranged at the enumeration. ' +
      'The schema declares `enums` AND `range`; both are canonicalization-inert — this hash is ' +
      'byte-identical to the one the same nodes produce under a schema carrying neither. ' +
      '`Condition` has no `classes` entry: an enumeration stands alone.',
    pkg,
    nodes: memberRefNodes,
  },
  {
    id: 'enum-member-ref-list',
    description:
      'A property is OPTIONAL and UNBOUNDED unless a shape says otherwise, so an enum-ranged ' +
      'property may legitimately carry SEVERAL member references. Each is a reference; none is ' +
      'embeddable. Included so the single-value case is not mistaken for the only valid shape.',
    pkg,
    nodes: [
      {
        $type: `${NS}/Product`,
        $id: `${D}/prod-3`,
        sku: 'SKU-3',
        condition: [
          { $ref: `${NS}/condition-new` },
          { $ref: `${NS}/condition-used` },
        ],
      } as CodecNode,
    ],
  },
  {
    id: 'enum-range-embedded-rejected',
    description:
      'An EMBEDDED value on an enum-ranged property must be rejected. An enum member is a ' +
      'reference, never an embeddable resource, and `Condition` has no `classes` entry to map ' +
      'fields against — so canonicalization fails loudly instead of inventing a mapping. This ' +
      'case is what keeps `range` inert for enum-ranged props: the inertness holds precisely ' +
      'because this path is unreachable. Rejection is required at CANONICALIZATION only — ' +
      'serialize is schema-free and deserialize does not recurse into embedded values.',
    pkg,
    expectError: true,
    nodes: [
      {
        $type: `${NS}/Product`,
        $id: `${D}/prod-2`,
        sku: 'SKU-2',
        condition: { grade: 'A' },
      } as CodecNode,
    ],
  },
];

// Fill expected values for the positive cases from the reference implementation.
for (const c of cases) {
  if (c.expectError) continue;
  c.expectedCanonicalForm = packageCanonicalForm(c.nodes, schema, c.pkg);
  c.expectedHash = packageContentHash(c.nodes, schema, c.pkg);
  c.expectedSerialize = c.nodes.map((n) => serialize(n));
}

// INERTNESS, enforced at generation time as well as in the vectors: the hash
// under the enums-bearing schema must equal the hash without it. If this ever
// trips, `enums`/`range` have become hash-relevant and the contract changed.
const withEnums = packageContentHash(memberRefNodes, schema, pkg);
const withoutEnums = packageContentHash(memberRefNodes, schemaWithoutEnums, pkg);
if (withEnums !== withoutEnums) {
  throw new Error(
    `enums/range are NOT canonicalization-inert: ${withEnums} vs ${withoutEnums}`,
  );
}

const out = {
  description:
    'Enumerations in the schema (`enums`, kanonak-codec 0.5.0, runtime#21): carried metadata ' +
    'that lets a consumer resolve a member URI without a generated SDK. Pins three things — ' +
    'that `enums` and an enum-ranged `range` are canonicalization-inert, that an enumeration ' +
    'stands alone with no `classes` twin, and that an embedded value on an enum-ranged ' +
    'property is rejected (the condition the inertness depends on).',
  canonicalFormVersion: '1',
  schema,
  cases,
};

const target = new URL('../codec-vectors-enums.json', import.meta.url);
writeFileSync(target, JSON.stringify(out, null, 1) + '\n', 'utf8');
console.log(`wrote ${target.pathname} — ${cases.length} cases, inertness verified (${withEnums})`);
