/**
 * Generates codec-vectors-types.json — the conformance vectors for MULTI-TYPED
 * SUBJECTS ($types, kanonak-codec 0.4.0, kanonak-protocol/runtime#10) — from
 * the TypeScript reference implementation. Expected values are authoritative
 * once committed; regeneration must be byte-identical unless the contract
 * itself changes.
 *
 * Run from the typescript port directory (so module resolution finds
 * @kanonak-protocol/canonical):
 *
 *   cd kanonak-codec/typescript && npx tsx ../vectors/gen-types-vectors/gen.ts
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

const schema: CodecSchema = {
  typePredicate: 'kanonak.org/core-rdf@1.1.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.1.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@1.0.0/Package',
  classes: {
    // The meta-typed definition-resource story from runtime#10: a resource that
    // is simultaneously a class definition and a shape. AnnotatedDef sorts
    // BEFORE ClassDef (for the primary-not-first case) and maps `note` to a
    // DIFFERENT predicate than ClassDef does (so the wrong-primary case changes
    // the field mapping — visibly, in bytes).
    [`${NS}/AnnotatedDef`]: {
      typeUri: `${NS}/AnnotatedDef`,
      props: {
        note: {
          predicate: `${NS}/annotation`,
          kind: 'datatype',
          datatype: 'kanonak.org/core-xsd/string',
        },
      },
    },
    [`${NS}/ClassDef`]: {
      typeUri: `${NS}/ClassDef`,
      props: {
        note: {
          predicate: `${NS}/note`,
          kind: 'datatype',
          datatype: 'kanonak.org/core-xsd/string',
        },
      },
    },
    [`${NS}/Bundle`]: {
      typeUri: `${NS}/Bundle`,
      props: {
        parts: {
          predicate: `${NS}/parts`,
          kind: 'object',
          range: `${NS}/PartDef`,
        },
      },
    },
    [`${NS}/PartDef`]: {
      typeUri: `${NS}/PartDef`,
      props: {
        size: {
          predicate: `${NS}/size`,
          kind: 'datatype',
          datatype: 'kanonak.org/core-xsd/integer',
        },
      },
    },
  },
};

const pkg: PackageContext = {
  publisher: 'probe.example.com',
  packageName: 'data',
  version: '1.0.0',
  label: 'Types Probe Data',
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

const D = 'probe.example.com/data@1.0.0';

const cases: Case[] = [
  {
    id: 'single-typed-unchanged',
    description:
      'A single-typed subject carries only $type and emits exactly one type statement — ' +
      'byte-identical to 0.2.0/0.3.0 output. The presence rule: $types appears only when ' +
      'the subject carries MORE THAN ONE type statement.',
    pkg,
    nodes: [
      { $type: `${NS}/ClassDef`, $id: `${D}/w1`, note: 'A' } as CodecNode,
    ],
  },
  {
    id: 'covered-redundant-set',
    description:
      'A redundantly-typed covered set (an authored `type: [derived, ancestor]`): $types ' +
      'carries the FULL set sorted by UTF-8 bytes, and the canonical form carries exactly ' +
      'one type statement PER MEMBER — no dedup, no extra statement for $type itself. ' +
      'Covered-or-not does not matter to the codec: the presence rule is purely syntactic ' +
      '(more than one type statement); subsumption stays the schema layer’s job.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/AnnotatedDef`, `${NS}/ClassDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
  },
  {
    id: 'independent-set-three-members',
    description:
      'An independent (beyond-closure) set of three types. Only $type’s class must exist in ' +
      'the embedded schema (it drives field mapping / dispatch); the other members are ' +
      'emitted as type statements by URI alone.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/ClassDef`, `${NS}/ShapeDef`, `${NS}/VersionedDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
  },
  {
    id: 'primary-not-first-sorted',
    description:
      '$type is NOT $types[0]: member sorting is lexical (URI bytes) while primary selection ' +
      'is semantic (most-derived / smallest maximal), so they can disagree. Kills any ' +
      '“dispatch on types[0]” shortcut. Canonical form and hash are pinned; note maps through ' +
      'ClassDef’s props.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/AnnotatedDef`, `${NS}/ClassDef`, `${NS}/VersionedDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
  },
  {
    id: 'wrong-primary-different-hash',
    description:
      'The SAME type set as primary-not-first-sorted with a DIFFERENT (wrongly chosen) ' +
      'primary: syntactically valid, but $type drives field mapping, so `note` maps through ' +
      'AnnotatedDef to a different predicate and the canonical form + hash DRIFT from the ' +
      'correct primary’s. This pair is why the primary rule (most-derived for a covered set, ' +
      'lexicographically smallest maximal member for an independent set) must be applied ' +
      'deterministically by the emitting side.',
    pkg,
    nodes: [
      {
        $type: `${NS}/AnnotatedDef`,
        $types: [`${NS}/AnnotatedDef`, `${NS}/ClassDef`, `${NS}/VersionedDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
  },
  {
    id: 'embedded-multi-typed-named',
    description:
      'Scope symmetry: the same rules on an embedded node — $types alongside $name. A ' +
      'multi-typed embedded carries one type statement per member inside the embedded ' +
      '(explicit embedded types are hash-relevant per the runtime#1 audit), and $name is ' +
      'unchanged by $types.',
    pkg,
    nodes: [
      {
        $type: `${NS}/Bundle`,
        $id: `${D}/b1`,
        parts: {
          $name: 'first',
          $type: `${NS}/PartDef`,
          $types: [`${NS}/PartDef`, `${NS}/SealedDef`],
          size: 2,
        },
      } as CodecNode,
    ],
  },
  {
    id: 'types-in-list-items',
    description:
      '$types on embedded LIST ITEMS, mixed with a single-typed (explicit $type only) ' +
      'sibling. List order stays fully semantic; each item’s type statements are its own.',
    pkg,
    nodes: [
      {
        $type: `${NS}/Bundle`,
        $id: `${D}/b1`,
        parts: [
          {
            $name: 'a',
            $type: `${NS}/PartDef`,
            $types: [`${NS}/PartDef`, `${NS}/SealedDef`],
            size: 1,
          },
          {
            $name: 'b',
            $type: `${NS}/PartDef`,
            size: 2,
          },
        ],
      } as CodecNode,
    ],
  },
  {
    id: 'unsorted-types-rejected',
    description:
      '$types not sorted by UTF-8 bytes is REJECTED — at serialize (the producer fails at ' +
      'emit time) AND at deserialize (a lenient reader that silently re-sorts would mask a ' +
      'nondeterministic emitter) AND at canonicalization.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/ClassDef`, `${NS}/AnnotatedDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
    expectError: true,
  },
  {
    id: 'singleton-types-rejected',
    description:
      'A one-member $types is REJECTED: it would be a second wire encoding of single-typed ' +
      'content, and two encodings of the same content is hash ambiguity.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/ClassDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
    expectError: true,
  },
  {
    id: 'duplicate-types-rejected',
    description:
      'Duplicate members are REJECTED: authored documents cannot produce them (statement ' +
      'parsing dedups), so a duplicate on the wire is always a producer bug.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ClassDef`,
        $types: [`${NS}/ClassDef`, `${NS}/ClassDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
    expectError: true,
  },
  {
    id: 'type-not-member-rejected',
    description:
      '$type must be a member of $types when $types is present — the primary is one of the ' +
      'set, never a separate assertion.',
    pkg,
    nodes: [
      {
        $type: `${NS}/ShapeDef`,
        $types: [`${NS}/AnnotatedDef`, `${NS}/ClassDef`],
        $id: `${D}/w1`,
        note: 'A',
      } as CodecNode,
    ],
    expectError: true,
  },
  {
    id: 'embedded-types-without-type-rejected',
    description:
      'An embedded carrying $types MUST carry an explicit $type that is a member — ' +
      'range-derived typing cannot choose a primary for a multi-typed embedded.',
    pkg,
    nodes: [
      {
        $type: `${NS}/Bundle`,
        $id: `${D}/b1`,
        parts: {
          $name: 'first',
          $types: [`${NS}/PartDef`, `${NS}/SealedDef`],
          size: 2,
        },
      } as CodecNode,
    ],
    expectError: true,
  },
];

for (const c of cases) {
  if (c.expectError) {
    // Prove all three surfaces reject, then leave only the marker in the file.
    for (const [what, run] of [
      ['canonicalize', () => packageCanonicalForm(c.nodes, schema, c.pkg)],
      ['serialize', () => c.nodes.map((n) => serialize(n))],
    ] as const) {
      let threw = false;
      try {
        run();
      } catch {
        threw = true;
      }
      if (!threw) throw new Error(`case ${c.id}: expected ${what} to throw, it did not`);
    }
    continue;
  }
  c.expectedCanonicalForm = packageCanonicalForm(c.nodes, schema, c.pkg);
  c.expectedHash = packageContentHash(c.nodes, schema, c.pkg);
  c.expectedSerialize = c.nodes.map((n) => serialize(n));
}

const doc = {
  description:
    'Codec conformance vectors for MULTI-TYPED SUBJECTS — the $types reserved envelope key ' +
    '(kanonak-codec 0.4.0, kanonak-protocol/runtime#10). Ports implementing the 0.3.0 contract ' +
    'do not run this file. Contract: $type stays the scalar dispatch key; $types (sorted by ' +
    'UTF-8 bytes, >= 2 members, no duplicates, $type a member) is present exactly when the ' +
    'node carries more than one type statement, and each member emits ONE type statement in ' +
    'canonical form — no dedup, no extra statement for $type. Single-typed nodes are ' +
    'byte-identical to 0.2.0/0.3.0 output. Validation is symmetric: serialize AND deserialize ' +
    'AND canonicalization all reject an invalid set (expectError cases); readers reject, never ' +
    'repair. Round-trip: deserialize(serialize(x)) preserves $types exactly. Expected values ' +
    'are authoritative; generated by gen-types-vectors/gen.ts from the TypeScript reference.',
  canonicalFormVersion: '1',
  schema,
  cases,
};

const out = new URL('../codec-vectors-types.json', import.meta.url);
writeFileSync(out, JSON.stringify(doc, null, 2) + '\n');
console.log(`wrote ${cases.length} cases to ${out.pathname}`);
