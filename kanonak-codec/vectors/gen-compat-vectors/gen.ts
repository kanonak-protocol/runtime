/**
 * Generates codec-vectors-compat.json — the conformance vectors for reading
 * nodes typed at an earlier COMPATIBLE version of the schema's own package
 * (kanonak-codec 0.6.1, kanonak-protocol/runtime#28) — from the TypeScript
 * reference implementation. Expected values are authoritative once committed;
 * regeneration must be byte-identical unless the contract itself changes.
 *
 * The contract, and what each section pins:
 *
 *  - deserializeCases: `deserialize` resolves `$type` through the exact
 *    versioned class first, then the same class at a version that can read the
 *    written one — `isReadableBy(written, schemaVersion)` from
 *    kanonak-canonical (from 1.0.0: same major, schema minor >= written minor,
 *    patch not compared; below 1.0.0: same minor, schema patch >= written
 *    patch). The decoded node keeps the `$type` it was WRITTEN with. Every
 *    rejection names its kind in brackets: [unknown-type], [newer-version],
 *    [other-major], [other-minor-line].
 *
 *  - typeMatchesCases: `typeMatches(node, classUri, schema)` — what a
 *    generated type guard asks. Each of the node's types resolves through the
 *    same compatible lookup; it matches when that class IS classUri or lists
 *    it in `ancestors` (the optional, canonicalization-inert schema field that
 *    makes a superclass check accept a subclass). Unreadable types never match.
 *
 *  - enumMemberCases: `enumMember(schema, ref)` — the member a `{"$ref": …}`
 *    names, exact key first, then the same member at a version that can read
 *    the written one. null means "not this schema's member" (as for `enums`
 *    itself: absence is never "invalid").
 *
 *  - hashCases: hashing is EXACT-VERSION ONLY. A content hash is computed over
 *    the producer's predicate and type URIs, versions included, so a node
 *    written at an earlier compatible version cannot be re-hashed with a later
 *    schema; when only a compatible class exists the hash surfaces fail with
 *    [hash-needs-exact-version] — for a node's `$type` and for an embedded
 *    value's. The exact case pins that nothing about hashing changed.
 *
 * Run from the typescript port directory:
 *
 *   cd kanonak-codec/typescript && npx tsx ../vectors/gen-compat-vectors/gen.ts
 */
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  deserialize,
  typeMatches,
  enumMember,
  packageContentHash,
  type CodecSchema,
  type CodecNode,
  type PackageContext,
} from '../../typescript/src/index.js';

const V = 'example.com/vocab';
const E = 'example.com/early';
const OTHER = 'other.example/tags@1.0.0/Tagged';
const DATA = 'data.example/apps@1.0.0';
const XSD_STRING = 'kanonak.org/core-xsd/string';

const at = (pkg: string, version: string, name: string) => `${pkg}@${version}/${name}`;

const fiveOne: CodecSchema = {
  typePredicate: 'kanonak.org/core-rdf@1.1.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.1.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@2.2.0/Package',
  classes: {
    [at(V, '5.1.0', 'Application')]: {
      typeUri: at(V, '5.1.0', 'Application'),
      props: {
        name: { predicate: at(V, '5.1.0', 'name'), kind: 'datatype', datatype: XSD_STRING },
        region: { predicate: at(V, '5.1.0', 'region'), kind: 'object', range: at(V, '5.1.0', 'Region') },
        components: { predicate: at(V, '5.1.0', 'components'), kind: 'object', range: at(V, '5.1.0', 'Application') },
      },
    },
    [at(V, '5.1.0', 'TerminalApplication')]: {
      typeUri: at(V, '5.1.0', 'TerminalApplication'),
      props: {
        name: { predicate: at(V, '5.1.0', 'name'), kind: 'datatype', datatype: XSD_STRING },
        shell: { predicate: at(V, '5.1.0', 'shell'), kind: 'datatype', datatype: XSD_STRING },
      },
      ancestors: [at(V, '5.1.0', 'Application')],
    },
  },
  enums: {
    [at(V, '5.1.0', 'Region')]: {
      typeUri: at(V, '5.1.0', 'Region'),
      members: {
        [at(V, '5.1.0', 'north')]: { label: 'North' },
        [at(V, '5.1.0', 'south')]: { label: 'South' },
      },
    },
  },
};

const zeroFiveTwo: CodecSchema = {
  typePredicate: 'kanonak.org/core-rdf@1.1.0/type',
  labelPredicate: 'kanonak.org/core-rdf@1.1.0/label',
  packageTypeUri: 'kanonak.org/core-kanonak@2.2.0/Package',
  classes: {
    [at(E, '0.5.2', 'Widget')]: {
      typeUri: at(E, '0.5.2', 'Widget'),
      props: { name: { predicate: at(E, '0.5.2', 'name'), kind: 'datatype', datatype: XSD_STRING } },
    },
  },
};

const schemas: Record<string, CodecSchema> = { 'five-one': fiveOne, 'zero-five-two': zeroFiveTwo };

/** The bracketed kind of a rejection, or a failure when there is none. */
function kindOf(err: unknown): string {
  const m = /\[([a-z-]+)\]$/.exec(err instanceof Error ? err.message : String(err));
  if (!m) throw new Error(`rejection without a bracketed kind: ${String(err)}`);
  return m[1]!;
}

// -- deserialize -------------------------------------------------------------------------
const app = (version: string, extra: Record<string, unknown> = {}) => ({
  $type: at(V, version, 'Application'), $id: `${DATA}/a1`, name: 'Editor', color: 'red', ...extra,
});
const widget = (version: string) => ({ $type: at(E, version, 'Widget'), $id: `${DATA}/w1`, name: 'Knob' });

const deserializeInputs: { id: string; schema: string; input: Record<string, unknown>; why: string }[] = [
  { id: 'exact', schema: 'five-one', input: app('5.1.0'), why: 'the exact versioned class; an unmodeled key lands in $extra' },
  { id: 'older-minor', schema: 'five-one', input: app('5.0.0'), why: 'written at 5.0.0, read with the 5.1.0 schema; the node keeps its written $type' },
  { id: 'older-minor-other-patch', schema: 'five-one', input: app('5.0.3'), why: 'the minor decides; the patch is not compared' },
  { id: 'newer-patch', schema: 'five-one', input: app('5.1.4'), why: 'from 1.0.0 a patch changes no term, so 5.1.4 data reads with the 5.1.0 schema' },
  { id: 'newer-minor', schema: 'five-one', input: app('5.2.0'), why: 'the data may use terms the 5.1.0 schema lacks' },
  { id: 'older-major', schema: 'five-one', input: app('4.2.0'), why: 'a different major is a different line' },
  { id: 'newer-major', schema: 'five-one', input: app('6.0.0'), why: 'a different major is a different line' },
  { id: 'unknown-class', schema: 'five-one', input: { $type: at(V, '5.1.0', 'Nope'), $id: `${DATA}/n1` }, why: 'no such class at any version' },
  { id: 'other-package', schema: 'five-one', input: { $type: 'other.example/vocab@5.1.0/Application', $id: `${DATA}/o1` }, why: 'same local name, another package' },
  { id: 'versionless-type', schema: 'five-one', input: { $type: `${V}/Application`, $id: `${DATA}/v1` }, why: 'a versionless $type has no version to check, so only an exact key could match' },
  { id: 'zero-x-older-patch', schema: 'zero-five-two', input: widget('0.5.0'), why: 'below 1.0.0 the patch carries additions: 0.5.2 reads 0.5.0' },
  { id: 'zero-x-newer-patch', schema: 'zero-five-two', input: widget('0.5.3'), why: 'below 1.0.0 a newer patch is a newer writer' },
  { id: 'zero-x-older-line', schema: 'zero-five-two', input: widget('0.4.9'), why: 'below 1.0.0 a different minor is a different line' },
  { id: 'zero-x-newer-line', schema: 'zero-five-two', input: widget('0.6.0'), why: 'below 1.0.0 a different minor is a different line' },
];
const deserializeCases = deserializeInputs.map((c) => {
  try {
    return { ...c, expected: deserialize(c.input, schemas[c.schema]!) };
  } catch (err) {
    return { ...c, expectError: kindOf(err) };
  }
});

// -- typeMatches ---------------------------------------------------------------------------
const typeMatchesInputs: { id: string; schema: string; node: Record<string, unknown>; classUri: string; why: string }[] = [
  { id: 'subclass-older-minor', schema: 'five-one', node: { $type: at(V, '5.0.0', 'TerminalApplication') }, classUri: at(V, '5.1.0', 'Application'), why: 'an Application check accepts a TerminalApplication written at an earlier compatible minor' },
  { id: 'same-class', schema: 'five-one', node: { $type: at(V, '5.1.0', 'TerminalApplication') }, classUri: at(V, '5.1.0', 'TerminalApplication'), why: 'the class itself' },
  { id: 'superclass-is-not-subclass', schema: 'five-one', node: { $type: at(V, '5.0.0', 'Application') }, classUri: at(V, '5.1.0', 'TerminalApplication'), why: 'an Application is not a TerminalApplication' },
  { id: 'newer-minor-never-matches', schema: 'five-one', node: { $type: at(V, '5.2.0', 'Application') }, classUri: at(V, '5.1.0', 'Application'), why: 'a type this schema cannot read never matches' },
  { id: 'multi-typed-member', schema: 'five-one', node: { $type: at(V, '5.0.0', 'Application'), $types: [at(V, '5.0.0', 'Application'), OTHER] }, classUri: at(V, '5.1.0', 'Application'), why: 'any member of $types may match' },
  { id: 'foreign-member', schema: 'five-one', node: { $type: at(V, '5.0.0', 'Application'), $types: [at(V, '5.0.0', 'Application'), OTHER] }, classUri: OTHER, why: 'a class this schema does not have is never matched' },
  { id: 'untyped-node', schema: 'five-one', node: {}, classUri: at(V, '5.1.0', 'Application'), why: 'a node with no type matches nothing' },
];
const typeMatchesCases = typeMatchesInputs.map((c) => ({
  ...c,
  expected: typeMatches(c.node as CodecNode, c.classUri, schemas[c.schema]!),
}));

// -- enumMember ------------------------------------------------------------------------------
const enumInputs: { id: string; schema: string; ref: string; why: string }[] = [
  { id: 'exact-member', schema: 'five-one', ref: at(V, '5.1.0', 'north'), why: 'the exact versioned key' },
  { id: 'older-minor-member', schema: 'five-one', ref: at(V, '5.0.0', 'north'), why: 'referenced at an earlier compatible minor' },
  { id: 'newer-minor-member', schema: 'five-one', ref: at(V, '5.2.0', 'north'), why: 'a member referenced at a newer minor is not this schema\'s' },
  { id: 'unknown-member', schema: 'five-one', ref: at(V, '5.1.0', 'east'), why: 'no such member' },
  { id: 'other-package-member', schema: 'five-one', ref: 'other.example/vocab@5.1.0/north', why: 'another package' },
  { id: 'versionless-member', schema: 'five-one', ref: `${V}/north`, why: 'a versionless reference has no version to check' },
];
const enumMemberCases = enumInputs.map((c) => {
  const m = enumMember(schemas[c.schema]!, c.ref);
  return { ...c, expected: m ? { enumType: m.enumType, uri: m.uri, label: m.member.label ?? null } : null };
});

// -- hashing ------------------------------------------------------------------------------------
const pkg: PackageContext = { publisher: 'data.example', packageName: 'apps', version: '1.0.0', label: 'Apps' };
const hashInputs: { id: string; schema: string; nodes: Record<string, unknown>[]; why: string }[] = [
  { id: 'exact-hashes', schema: 'five-one', nodes: [{ $type: at(V, '5.1.0', 'Application'), $id: `${DATA}/a1`, name: 'Editor' }], why: 'the exact class: hashing is unchanged' },
  { id: 'compatible-node-cannot-hash', schema: 'five-one', nodes: [{ $type: at(V, '5.0.0', 'Application'), $id: `${DATA}/a1`, name: 'Editor' }], why: 'the predicates would carry 5.1.0 where the producer hashed 5.0.0' },
  {
    id: 'compatible-embedded-cannot-hash',
    schema: 'five-one',
    nodes: [{
      $type: at(V, '5.1.0', 'Application'), $id: `${DATA}/a2`, name: 'Suite',
      components: [{ $type: at(V, '5.0.0', 'TerminalApplication'), name: 'Shell' }],
    }],
    why: 'the same holds for an embedded value written at the earlier minor',
  },
  { id: 'unknown-cannot-hash', schema: 'five-one', nodes: [{ $type: at(V, '5.1.0', 'Nope'), $id: `${DATA}/n1` }], why: 'no such class' },
];
const hashCases = hashInputs.map((c) => {
  try {
    return { ...c, expectedHash: packageContentHash(c.nodes as CodecNode[], schemas[c.schema]!, pkg) };
  } catch (err) {
    return { ...c, expectError: kindOf(err) };
  }
});

const doc = {
  description:
    'Reading nodes typed at an earlier COMPATIBLE version of the schema\'s own package (kanonak-codec 0.6.1, runtime#28). ' +
    'deserialize resolves $type exactly, then through isReadableBy (kanonak-canonical) to the same class at a version that can read it, ' +
    'and keeps the written $type; typeMatches answers a generated guard the same way, subclass-aware through the optional, ' +
    'canonicalization-inert `ancestors` field; enumMember resolves a $ref the same way; hashing stays exact-version only. ' +
    'Every rejection message ends in a bracketed kind, which a port must reproduce: [unknown-type], [newer-version], ' +
    '[other-major], [other-minor-line], [hash-needs-exact-version]. See gen-compat-vectors/gen.ts for the full contract.',
  canonicalFormVersion: '1',
  pkg,
  schemas,
  deserializeCases,
  typeMatchesCases,
  enumMemberCases,
  hashCases,
};

const out = fileURLToPath(new URL('../codec-vectors-compat.json', import.meta.url));
writeFileSync(out, JSON.stringify(doc, null, 2) + '\n', 'utf8');
console.log(`wrote ${out}: ${deserializeCases.length} deserialize, ${typeMatchesCases.length} typeMatches, ${enumMemberCases.length} enumMember, ${hashCases.length} hash`);
