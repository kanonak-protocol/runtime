/**
 * Generates codec-vectors-references.json — the conformance vectors for
 * REFERENCE VALUES (kanonak-protocol/runtime#6): a `{"$ref": uri}` must address
 * a NAMED resource, `publisher/package[@version]/name`, and a URI fragment is
 * rejected at canonicalization, serialize AND deserialize. Same file format as
 * codec-vectors-types.json, so every port runs it with its existing types
 * runner.
 *
 * Every rejected case has a TWIN — the same nodes with the fragment removed —
 * that this generator proves is accepted, so a port passing the file rejects
 * the fragment and nothing else. Expected values are authoritative once
 * committed; regeneration must be byte-identical unless the contract changes.
 *
 *   cd kanonak-codec/typescript && npx tsx ../vectors/gen-references-vectors/gen.ts
 */
import { writeFileSync } from 'node:fs';
import {
  deserialize,
  packageCanonicalForm,
  packageContentHash,
  serialize,
  type CodecNode,
  type CodecSchema,
  type PackageContext,
} from '../../typescript/src/index.js';

const NS = 'probe.example.com/schema@1.0.0';
const D = 'probe.example.com/data@1.0.0';

const schema: CodecSchema = {
  typePredicate: 'kanonak.org/core-rdf@1.1.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.1.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@1.0.0/Package',
  classes: {
    [`${NS}/Doc`]: {
      typeUri: `${NS}/Doc`,
      props: {
        title: { predicate: `${NS}/title`, kind: 'datatype', datatype: 'kanonak.org/core-xsd/string' },
        cites: { predicate: `${NS}/cites`, kind: 'object', range: `${NS}/Doc` },
        sections: { predicate: `${NS}/sections`, kind: 'object', range: `${NS}/Section` },
      },
    },
    [`${NS}/Section`]: {
      typeUri: `${NS}/Section`,
      props: {
        heading: { predicate: `${NS}/heading`, kind: 'datatype', datatype: 'kanonak.org/core-xsd/string' },
        cites: { predicate: `${NS}/cites`, kind: 'object', range: `${NS}/Doc` },
      },
    },
  },
};

const pkg: PackageContext = {
  publisher: 'probe.example.com',
  packageName: 'data',
  version: '1.0.0',
  label: 'References Probe Data',
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

const doc = (cites: unknown, extra: Record<string, unknown> = {}): CodecNode =>
  ({ $type: `${NS}/Doc`, $id: `${D}/d1`, title: 'One', cites, ...extra }) as CodecNode;

const accepted: Case[] = [
  {
    id: 'named-reference-versioned',
    description: 'A reference to a named resource at a version: accepted, hashed as a reference statement.',
    pkg,
    nodes: [doc({ $ref: `${D}/d2` })],
  },
  {
    id: 'named-reference-versionless',
    description: 'A versionless named reference (publisher/package/name) is still a named resource: accepted.',
    pkg,
    nodes: [doc({ $ref: 'probe.example.com/data/d2' })],
  },
  {
    id: 'named-references-in-list',
    description: 'A list of named references: every item accepted, list order hashed.',
    pkg,
    nodes: [doc([{ $ref: `${D}/d2` }, { $ref: `${D}/d3` }])],
  },
  {
    id: 'named-reference-in-embedded',
    description: 'A named reference inside an embedded value: accepted at any depth.',
    pkg,
    nodes: [doc(undefined, { sections: [{ $name: 'intro', heading: 'Intro', cites: { $ref: `${D}/d2` } }] })],
  },
];

const rejected: (Case & { twin: string })[] = [
  {
    id: 'fragment-reference-rejected',
    twin: 'named-reference-versioned',
    description:
      'A reference into an embedded resource by URI fragment is REJECTED on all three surfaces. ' +
      'Fragments address embedded resources for navigation; they are not part of the reference graph.',
    pkg,
    nodes: [doc({ $ref: `${D}/d2#sections[0]` })],
    expectError: true,
  },
  {
    id: 'fragment-on-versionless-reference-rejected',
    twin: 'named-reference-versionless',
    description: 'A fragment on a versionless reference is rejected the same way.',
    pkg,
    nodes: [doc({ $ref: 'probe.example.com/data/d2#intro' })],
    expectError: true,
  },
  {
    id: 'fragment-reference-in-list-rejected',
    twin: 'named-references-in-list',
    description: 'One fragment reference anywhere in a list rejects the node; the named items do not excuse it.',
    pkg,
    nodes: [doc([{ $ref: `${D}/d2` }, { $ref: `${D}/d3#sections[1]` }])],
    expectError: true,
  },
  {
    id: 'fragment-reference-in-embedded-rejected',
    twin: 'named-reference-in-embedded',
    description: 'A fragment reference inside an embedded value is rejected: the rule holds at any depth.',
    pkg,
    nodes: [doc(undefined, { sections: [{ $name: 'intro', heading: 'Intro', cites: { $ref: `${D}/d2#intro` } }] })],
    expectError: true,
  },
];

function throws(run: () => unknown): boolean {
  try {
    run();
    return false;
  } catch (err) {
    if (!(err as Error).message.endsWith('[fragment-reference]')) {
      throw new Error(`rejected for the wrong reason: ${(err as Error).message}`);
    }
    return true;
  }
}

const strip = (nodes: CodecNode[]): CodecNode[] =>
  JSON.parse(JSON.stringify(nodes, (_k, v) => (_k === '$ref' && typeof v === 'string' ? v.replace(/#.*$/, '') : v)));

for (const c of accepted) {
  c.expectedCanonicalForm = packageCanonicalForm(c.nodes, schema, c.pkg);
  c.expectedHash = packageContentHash(c.nodes, schema, c.pkg);
  c.expectedSerialize = c.nodes.map((n) => serialize(n));
}
for (const c of rejected) {
  // All three surfaces reject, for the fragment and only the fragment.
  for (const [what, run] of [
    ['canonicalize', () => packageCanonicalForm(c.nodes, schema, c.pkg)],
    ['serialize', () => c.nodes.map((n) => serialize(n))],
    ['deserialize', () => c.nodes.map((n) => deserialize(n as Record<string, unknown>, schema))],
  ] as const) {
    if (!throws(run)) throw new Error(`case ${c.id}: expected ${what} to reject, it did not`);
  }
  const twin = accepted.find((a) => a.id === c.twin);
  if (!twin || JSON.stringify(strip(c.nodes)) !== JSON.stringify(twin.nodes)) {
    throw new Error(`case ${c.id}: its nodes minus the fragment must equal twin ${c.twin}`);
  }
}

const out = {
  description:
    'Codec conformance vectors for REFERENCE VALUES (kanonak-protocol/runtime#6). A reference value ' +
    '({"$ref": uri}) MUST address a named resource, publisher/package[@version]/name; a URI fragment ' +
    'is a navigation convention, not part of the reference graph, and is REJECTED at canonicalization, ' +
    'serialize AND deserialize, at any depth (expectError cases; the TypeScript reference ends the ' +
    'message in [fragment-reference]). Every rejected case equals an accepted case with the fragment ' +
    'removed, so the fragment is the only reason to reject. Same format as codec-vectors-types.json. ' +
    'Expected values are authoritative; generated by gen-references-vectors/gen.ts from the TypeScript ' +
    'reference.',
  canonicalFormVersion: '1',
  schema,
  cases: [...accepted, ...rejected.map(({ twin: _twin, ...c }) => c)],
};

const file = new URL('../codec-vectors-references.json', import.meta.url);
writeFileSync(file, JSON.stringify(out, null, 2) + '\n');
console.log(`wrote ${out.cases.length} cases (${accepted.length} accepted, ${rejected.length} rejected)`);
