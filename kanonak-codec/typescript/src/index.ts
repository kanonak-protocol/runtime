/**
 * `@kanonak-protocol/codec` — the generic, ontology-INDEPENDENT codec runtime.
 *
 * Given a `CodecSchema` (the per-package metadata a generated SDK embeds) and a
 * set of typed nodes, it builds the language-neutral `CanonicalInput` model and
 * content-addresses it via `@kanonak-protocol/canonical` (`canonicalHash`). The
 * same logic backs every generated package — only the embedded `CodecSchema`
 * differs — so it lives here as one audited, vector-checked dependency rather
 * than re-inlined per package.
 *
 * Self-contained: carriers come from the schema's datatype URIs (no runtime
 * ontology resolution), and the resolved foundation URIs (`type`/`label`
 * predicates, the `Package` class) are embedded by the generator. The output
 * therefore byte-matches `kanonak hash` of the equivalent authored package,
 * including the synthesized `rdf:type` triples and the package-wrapper subject.
 *
 * This is the TypeScript port of the codec runtime; the C#/Rust/Go/Java/Python
 * ports reference their `kanonak-canonical` library for the same `canonicalHash`
 * entry — TypeScript references `@kanonak-protocol/canonical`. The only depend-
 * ency is that one lightweight canonical-form library.
 */
import {
  canonicalForm,
  canonicalHash,
  carrierOf,
  Carrier,
  parseCoordinate,
  isReadableBy,
  lenientVersionlessKey,
  canonicalInteger,
  canonicalDecimal,
} from '@kanonak-protocol/canonical';
import type { CanonicalInput, CanonicalInputStatement, CanonicalInputValue, CoordinateVersion } from '@kanonak-protocol/canonical';

/** Reserved `$`-envelope keys — never emitted as ontology statements. */
const ENVELOPE_KEYS = new Set(['$type', '$types', '$id', '$name', '$contentHash', '$version', '$extra']);

const UTF8 = new TextEncoder();

/** Lexicographic comparison by UTF-8 byte sequence (== Unicode code-point order). */
function compareUtf8(a: string, b: string): number {
  const ab = UTF8.encode(a);
  const bb = UTF8.encode(b);
  const n = Math.min(ab.length, bb.length);
  for (let i = 0; i < n; i++) {
    if (ab[i] !== bb[i]) return ab[i] - bb[i];
  }
  return ab.length - bb.length;
}

/** One property's canonicalization metadata, as embedded by the generator. */
export interface CodecProp {
  /** The predicate's durable canonical URI (with resolved version). */
  predicate: string;
  /** Datatype vs object — decides typed-scalar vs reference/embedded. */
  kind: 'datatype' | 'object';
  /** The datatype's canonical URI (carrier source) — present for datatype props. */
  datatype?: string;
  /**
   * The range class's canonical URI — present for object props (0.2.0). Maps an
   * embedded value's fields when the embedded carries no explicit `$type`
   * (range-derived typing: inference only, never materialized as a statement).
   */
  range?: string;
}

/**
 * One member of a closed set of named individuals.
 *
 * An object even when `label` is its only field, so a later addition (an
 * ordered scale's `dominates` closure, runtime#16) stays additive rather than
 * a breaking shape change.
 */
export interface CodecEnumMember {
  /**
   * The member's own label — the ontology's, read through the same predicate
   * {@link CodecSchema.labelPredicate} names. Display-lens resolution is a
   * RENDERING concern and deliberately not here; anything presenting this can
   * layer a lens above it.
   */
  label?: string;
}

/**
 * A closed set of named individuals — the metadata that lets a consumer
 * holding only the schema resolve a member URI it received on the wire, with
 * no generated SDK for the declaring package.
 *
 * What makes a set closed is the PRODUCER's decision, taken in the ontology
 * and resolved by the schema layer above. This runtime is deliberately
 * ontology-independent: it carries the outcome and names no vocabulary term.
 *
 * Carried, never enforced. A prop ranged at an enumeration in ANOTHER package
 * has no entry here — `classes` and `enums` both describe the SDK's OWN
 * package — so a codec that validated a `$ref` against these members would
 * false-reject valid cross-package data. Absence means "not mine", never
 * "invalid".
 */
export interface CodecEnum {
  /** The enumeration class's durable URI — the same key form `classes` uses. */
  typeUri: string;
  /**
   * Members keyed by durable VERSIONED URI — `publisher/package@version/name`,
   * byte-identical to what the wire carries as `{"$ref": …}` and to the key
   * form `classes` uses. A versionless key would look right and miss every
   * lookup, so the formation is an invariant, not a convention.
   *
   * The local name is NOT duplicated as a key: derive it from the URI with
   * canonical's `localName` (runtime#17), which is already pinned in every
   * port. Two spellings of one fact is a way for them to disagree.
   */
  members: Record<string, CodecEnumMember>;
}

/** A class's canonicalization schema: its durable URI + its (flattened) props. */
export interface CodecClass {
  /** The class's durable canonical URI — the value of the synthesized type triple. */
  typeUri: string;
  /** Properties keyed by local name (the wire field name). */
  props: Record<string, CodecProp>;
  /**
   * Every superclass of the class, transitively, by durable VERSIONED URI
   * (0.6.1, runtime#28) — what lets {@link typeMatches} accept a subclass
   * node for a superclass check (an `Application` check accepts a
   * `TerminalApplication`). Optional and additive: a schema without it is
   * still valid, and canonicalization never reads it. Absent, a type match is
   * by the class itself only — stricter, never looser.
   */
  ancestors?: string[];
}

/**
 * The metadata a generated SDK embeds for its codec — describes the SDK's OWN
 * classes (keyed by their durable URIs) + the resolved foundation URIs. It does
 * NOT carry a package identity: the classes belong to the SDK's package, but the
 * instances a consumer builds live in the consumer's own (data) package, whose
 * identity is supplied at call time via {@link PackageContext}.
 */
export interface CodecSchema {
  /** Resolved `kanonak.org/core-rdf@<ver>/type` predicate URI. */
  typePredicate: string;
  /** Resolved `kanonak.org/core-rdf@<ver>/label` predicate URI. */
  labelPredicate: string;
  /** Resolved `kanonak.org/core-kanonak@<ver>/Package` class URI. */
  packageTypeUri: string;
  /** Classes keyed by durable type URI (the node's `$type`). */
  classes: Record<string, CodecClass>;
  /**
   * Closed sets of named individuals, keyed by durable type URI (0.5.0,
   * runtime#21).
   *
   * Optional, so it is additive: a schema written before this field remains
   * valid, and every 0.4.1 reader already ignores it (measured across all
   * ports). Canonicalization-INERT — content hashes are unchanged by its
   * presence, which the enums vectors pin rather than assert.
   *
   * An entry here does NOT require a twin in {@link CodecSchema.classes}: a
   * `classes` entry is needed only when instances of that class are emitted
   * as NODES, which for an enumeration's members they are not. That absence
   * is load-bearing — it is what makes an embedded value on an enum-ranged
   * property fail loudly instead of being mapped as if it were a resource.
   */
  enums?: Record<string, CodecEnum>;
}

/**
 * A typed node: the `$`-envelope plus alias-collapsed local-name fields.
 *
 * No index signature — a generated interface (which has none) would not be
 * assignable to one that does. Concrete generated types (e.g. `Account`) are
 * therefore assignable to `CodecNode` directly; the runtime reads their dynamic
 * fields via `Object.entries`, casting to a record only where a key is written.
 */
export interface CodecNode {
  $type?: string;
  /**
   * The FULL type set of a multi-typed node (0.4.0, runtime#10) — present
   * whenever the node carries MORE THAN ONE type statement, absent on
   * single-typed nodes (whose only type is `$type`). Hash-relevant: each member
   * emits one type statement in canonical form (no dedup, no extra statement
   * for `$type` itself). Invariants, enforced at serialize, deserialize, and
   * canonicalization time: sorted by UTF-8 bytes, at least two members, no
   * duplicates, and `$type` (the dispatch key, chosen by the schema layer's
   * primary rule) is a member. `$type` is NOT necessarily `$types[0]` —
   * sorting is lexical, primary selection is semantic.
   */
  $types?: string[];
  $id?: string;
  $extra?: Record<string, unknown>;
}

// -- The typed surface (0.3.0) ----------------------------------------------
//
// TypeScript typed models ARE wire-form objects (structural typing), so the
// typed surface is types + arm constructors — no binding layer needed: a
// KanonakNode-shaped object passes to the codec entry points directly.

/**
 * The full `$`-envelope as data — what a generated typed model's root carries.
 * `$name` is an embedded value's authored dict-key and is HASH-RELEVANT
 * (serialized into the canonical form); null/absent for subjects.
 *
 * A multi-typed node's set is exposed ONLY as `$types` (inherited from
 * {@link CodecNode}) — deliberately no unprefixed `types` accessor, because an
 * ontology can model a property literally named `types`; the `$` prefix exists
 * to avoid exactly that collision.
 */
export interface KanonakNode extends CodecNode {
  $name?: string;
  $contentHash?: string;
  $version?: string;
}

/**
 * An object property's value: EXACTLY ONE of a reference to a named resource
 * (`{ $ref: uri }`) or an embedded node (the value inline — derived identity,
 * no `$id`). The choice between the arms is authorial and hash-relevant, so
 * generated models carry it in the type, never infer it.
 */
export type Ref<T> = { $ref: string } | (T & { $type?: string });

/** Type guard: the reference arm of a {@link Ref}. */
export function isRef<T>(value: Ref<T>): value is { $ref: string } {
  return typeof value === 'object' && value !== null && '$ref' in value;
}

/** A reference to a named resource by its canonical URI. */
export function ref(uri: string): { $ref: string } {
  if (!uri) throw new Error('A reference needs a canonical URI.');
  return { $ref: uri };
}

/**
 * A reference to a named resource by the instance itself — resolved through
 * the target's `$id`. The target must already carry its identity; an embedded
 * (id-less) value cannot be referenced.
 */
export function refTo(target: KanonakNode): { $ref: string } {
  if (!target.$id) {
    throw new Error(
      'refTo(target) requires a node with a non-empty $id — ' +
        'to carry the value inline instead, use embed(value).'
    );
  }
  return { $ref: target.$id };
}

/**
 * An embedded value, carried inline (derived identity — it must not have a
 * `$id`), optionally with its authored dict-key name (hash-relevant).
 */
export function embed<T extends KanonakNode>(value: T, name?: string): T {
  if (value.$id) {
    throw new Error(
      'An embedded value must not carry $id — to point at a named resource, use ref/refTo.'
    );
  }
  if (name !== undefined) value.$name = name;
  return value;
}

/**
 * A typed instance's codec node. Provided for cross-port parity — in
 * TypeScript the typed object already IS the node (the round through
 * {@link serialize} + {@link deserialize} normalizes `$extra` placement and
 * drops undefined, which the codec entry points tolerate anyway).
 */
export function toNode(typed: KanonakNode, schema: CodecSchema): CodecNode {
  return deserialize(serialize(typed), schema);
}

/**
 * The identity of the (data) package being content-addressed — the consumer's
 * package the nodes are assembled into. Used to synthesize the package-wrapper
 * subject `<publisher>/<packageName>@<version>/<packageName>`.
 */
export interface PackageContext {
  publisher: string;
  packageName: string;
  version: string;
  /** Optional package label (a raw/untyped string statement, as the parser emits). */
  label?: string;
}

/** A scalar's raw lexical token — the precise input the canonical entry normalizes. */
function lexical(value: unknown): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'bigint') return value.toString();
  return String(value);
}

/**
 * A JavaScript number holds every integer exactly only up to 2^53, so an integer or decimal carrier refuses a number
 * past it rather than hash a neighbouring value (runtime#37); a bigint or a string carries any integer exactly.
 */
function exactLexical(prop: CodecProp, raw: unknown): string {
  if (typeof raw === 'number') {
    const carrier = carrierOf(prop.datatype!);
    const unsafe =
      (carrier === Carrier.Integer && Number.isInteger(raw) && !Number.isSafeInteger(raw)) ||
      (carrier === Carrier.Decimal && Math.abs(raw) > Number.MAX_SAFE_INTEGER);
    if (unsafe) {
      throw new Error(
        `${prop.predicate}: the number ${raw} is past 2^53, where a JavaScript number can't hold every integer ` +
          'exactly; pass it as a bigint or a string [unsafe-number]',
      );
    }
  }
  return lexical(raw);
}

/**
 * THE class lookup (runtime#28). A node is typed with the version of the
 * class its producer's import resolved to; a codec generated from a later
 * COMPATIBLE version of the same package must still read it. So: the exact
 * versioned key first, then the class with the same publisher, package and
 * name whose version can read the written one — `isReadableBy` from
 * `@kanonak-protocol/canonical`, the protocol's single compatibility rule,
 * pinned in every port. When nothing matches, the error says why, with a
 * bracketed kind the conformance vectors pin: `[unknown-type]`,
 * `[newer-version]` (the data may use terms this schema lacks),
 * `[other-major]`, or `[other-minor-line]` (below 1.0.0 the minor is the
 * incompatible line).
 */
function classFor(schema: CodecSchema, typeUri: string): { cls: CodecClass } | { error: string } {
  const exact = schema.classes[typeUri];
  if (exact) return { cls: exact };
  const written = coordinateOf(typeUri);
  if (!written?.version) return { error: `no schema for type ${typeUri} [unknown-type]` };
  const key = `${written.publisher}/${written.package_}/${written.name}`;

  let readable: { cls: CodecClass; version: CoordinateVersion } | undefined;
  let nearest: CoordinateVersion | undefined;
  for (const [uri, cls] of Object.entries(schema.classes)) {
    const c = coordinateOf(uri);
    if (!c?.version || `${c.publisher}/${c.package_}/${c.name}` !== key) continue;
    if (isReadableBy(written.version, c.version)) {
      if (!readable || compareVersions(c.version, readable.version) > 0) readable = { cls, version: c.version };
    } else if (!nearest || compareVersions(c.version, nearest) > 0) {
      nearest = c.version;
    }
  }
  if (readable) return { cls: readable.cls };
  if (!nearest) return { error: `no schema for type ${typeUri} [unknown-type]` };
  return { error: incompatibleVersion(typeUri, written.version, nearest) };
}

function incompatibleVersion(typeUri: string, w: CoordinateVersion, r: CoordinateVersion): string {
  const ws = formatVersion(w);
  const rs = formatVersion(r);
  if (w.major !== r.major) {
    return `${typeUri} is written at ${ws}, a different major version than this codec's schema (${rs}) [other-major]`;
  }
  if (w.major === 0 && w.minor !== r.minor) {
    return `${typeUri} is written at ${ws}; below 1.0.0 a different minor is a different version line than this codec's schema (${rs}) [other-minor-line]`;
  }
  return `${typeUri} is written at ${ws}, newer than this codec's schema (${rs}); upgrade the codec to read it [newer-version]`;
}

/** The parsed coordinate, or undefined when the string is not one (never throws). */
function coordinateOf(uri: string): ReturnType<typeof parseCoordinate> | undefined {
  try {
    return parseCoordinate(uri);
  } catch {
    return undefined;
  }
}

function compareVersions(a: CoordinateVersion, b: CoordinateVersion): number {
  return a.major - b.major || a.minor - b.minor || a.patch - b.patch;
}

function formatVersion(v: CoordinateVersion): string {
  return `${v.major}.${v.minor}.${v.patch}`;
}

/**
 * The class to HASH a node or embedded value with: the exact versioned class
 * only. A content hash is computed over the producer's predicate and type
 * URIs, versions included, so a node written at an earlier compatible version
 * cannot be re-hashed with a later schema — its predicates would carry the
 * later version and the hash would differ. When only a compatible class
 * exists, say so (`[hash-needs-exact-version]`) rather than produce a
 * different hash or the bare "no schema" error.
 */
function hashClassFor(schema: CodecSchema, typeUri: string, what: string): CodecClass {
  const exact = schema.classes[typeUri];
  if (exact) return exact;
  const match = classFor(schema, typeUri);
  if ('cls' in match) {
    throw new Error(
      `Cannot hash ${what} ${typeUri}: this codec's schema has it at ${match.cls.typeUri}, and a content hash is ` +
        `computed with the producer's schema version — hashing needs the exact class [hash-needs-exact-version]`,
    );
  }
  throw new Error(`No schema for ${what} ${typeUri} [unknown-type]`);
}

/**
 * Whether a node is an instance of the class `classUri` — what a generated
 * type guard asks (runtime#28). Each of the node's types (`$types`, else its
 * `$type`) is resolved through the same compatible lookup `deserialize` uses,
 * so a node written at an earlier compatible version satisfies the later
 * version's guard; it matches when that class IS `classUri` or has it among
 * its `ancestors`. Classes compare by versionless identity. A type this
 * schema cannot read (unknown, newer, another major) never matches.
 */
export function typeMatches(
  node: { $type?: string; $types?: string[] },
  classUri: string,
  schema: CodecSchema,
): boolean {
  const target = coordinateOf(classUri);
  if (!target) throw new Error(`typeMatches: '${classUri}' is not a class coordinate`);
  const targetKey = `${target.publisher}/${target.package_}/${target.name}`;
  const keyOf = (uri: string): string | undefined => {
    const c = coordinateOf(uri);
    return c ? `${c.publisher}/${c.package_}/${c.name}` : undefined;
  };
  for (const member of node.$types ?? (node.$type ? [node.$type] : [])) {
    const match = classFor(schema, member);
    if (!('cls' in match)) continue;
    if (keyOf(match.cls.typeUri) === targetKey) return true;
    if (match.cls.ancestors?.some((a) => keyOf(a) === targetKey)) return true;
  }
  return false;
}

/** An enumeration member found by {@link enumMember}. */
export interface EnumMemberMatch {
  /** The enumeration class's durable URI (its key in `schema.enums`). */
  enumType: string;
  /** The member's durable URI as this schema keys it. */
  uri: string;
  member: CodecEnumMember;
}

/**
 * The enumeration member a `{"$ref": …}` names (runtime#28): the exact
 * versioned key first, then the same member at a version that can read the
 * written one — so a member referenced at an earlier compatible version of the
 * package resolves against a later schema. Undefined when this schema has no
 * such member, which, as for `enums` itself, means "not mine", never
 * "invalid".
 */
export function enumMember(schema: CodecSchema, ref: string): EnumMemberMatch | undefined {
  const enums = schema.enums ?? {};
  for (const [enumType, e] of Object.entries(enums)) {
    const member = e.members[ref];
    if (member) return { enumType, uri: ref, member };
  }
  const written = coordinateOf(ref);
  if (!written?.version) return undefined;
  const key = `${written.publisher}/${written.package_}/${written.name}`;
  let best: (EnumMemberMatch & { version: CoordinateVersion }) | undefined;
  for (const [enumType, e] of Object.entries(enums)) {
    for (const [uri, member] of Object.entries(e.members)) {
      const c = coordinateOf(uri);
      if (!c?.version || `${c.publisher}/${c.package_}/${c.name}` !== key) continue;
      if (!isReadableBy(written.version, c.version)) continue;
      if (!best || compareVersions(c.version, best.version) > 0) best = { enumType, uri, member, version: c.version };
    }
  }
  return best ? { enumType: best.enumType, uri: best.uri, member: best.member } : undefined;
}

/**
 * Validate a node-or-embedded's `$types` envelope and return the validated set,
 * or undefined when the node is single-typed (no `$types`). Enforced wherever
 * the envelope is touched — serialize, deserialize, and canonicalization — so a
 * producer fails at emit time and a reader never masks a nondeterministic
 * emitter by silently repairing (re-sorting, deduping) the set.
 */
function validatedTypes(
  map: { $type?: unknown; $types?: unknown },
  where: string
): string[] | undefined {
  const raw = map.$types;
  if (raw === undefined || raw === null) return undefined;
  if (!Array.isArray(raw) || raw.some((m) => typeof m !== 'string' || m.length === 0)) {
    throw new Error(`${where}: $types must be an array of non-empty type URIs`);
  }
  const types = raw as string[];
  if (types.length < 2) {
    throw new Error(
      `${where}: $types with ${types.length} member(s) is forbidden — a single-typed ` +
        'node carries only $type (a second encoding of the same content would be hash-ambiguous)'
    );
  }
  for (let i = 1; i < types.length; i++) {
    const cmp = compareUtf8(types[i - 1], types[i]);
    if (cmp === 0) {
      throw new Error(`${where}: $types carries duplicate member ${types[i]}`);
    }
    if (cmp > 0) {
      throw new Error(
        `${where}: $types is not sorted by UTF-8 bytes ` +
          `(${types[i - 1]} sorts after ${types[i]}) — ordering is the producer's job, never the reader's`
      );
    }
  }
  const primary = map.$type;
  if (typeof primary !== 'string' || !types.includes(primary)) {
    throw new Error(
      `${where}: $type (${typeof primary === 'string' ? primary : String(primary)}) ` +
        'must be present and a member of $types'
    );
  }
  return types;
}

/**
 * Recursively validate every `$types` envelope in a wire value (the node itself
 * and any embedded node at any depth). Shared by {@link serialize} (the
 * producer throws at emit time) and {@link deserialize} (the strict reader
 * rejects rather than repairs).
 */
function assertTypesEnvelopes(value: unknown, where: string): void {
  if (Array.isArray(value)) {
    value.forEach((item, i) => assertTypesEnvelopes(item, `${where}[${i}]`));
    return;
  }
  if (value && typeof value === 'object') {
    const map = value as Record<string, unknown>;
    if ('$types' in map) validatedTypes(map, where);
    for (const [k, v] of Object.entries(map)) {
      if (k !== '$types') assertTypesEnvelopes(v, `${where}.${k}`);
    }
  }
}

/**
 * Every reference value (`{"$ref": uri}`) in a wire value, at any depth, must
 * address a NAMED resource: `publisher/package[@version]/name`, no URI fragment
 * (runtime#6). Fragments address embedded resources for navigation and
 * deep-linking; they are not part of the reference graph, and accepting one
 * here would establish that capability by accident. Shared by
 * {@link serialize}, {@link deserialize} and canonicalization, so all three
 * reject the same input.
 */
function assertReferenceValues(value: unknown, where: string): void {
  if (Array.isArray(value)) {
    value.forEach((item, i) => assertReferenceValues(item, `${where}[${i}]`));
    return;
  }
  if (value && typeof value === 'object') {
    const map = value as Record<string, unknown>;
    if (typeof map.$ref === 'string') assertNamedReference(map.$ref, where);
    for (const [k, v] of Object.entries(map)) assertReferenceValues(v, `${where}.${k}`);
  }
}

function assertNamedReference(uri: string, where: string): void {
  if (uri.includes('#')) {
    throw new Error(
      `${where}: reference value '${uri}' contains a URI fragment. A reference addresses a named ` +
        `resource (publisher/package[@version]/name); fragments are a navigation convention, not ` +
        `part of the reference graph [fragment-reference]`,
    );
  }
}

/** Build the `CanonicalInputValue` for one (already array-unwrapped) field value. */
function valueOf(prop: CodecProp, raw: unknown, schema: CodecSchema): CanonicalInputValue {
  if (prop.kind === 'object') {
    // A node: a reference (`{ $ref }`) or an embedded resource.
    if (raw && typeof raw === 'object') {
      const map = raw as Record<string, unknown>;
      if ('$ref' in map) return { ref: String(map.$ref) };
      return embeddedValue(prop, map, schema);
    }
    throw new Error(
      `Object property ${prop.predicate} expects a reference ({ $ref }) or an ` +
        `embedded node (a map), got ${typeof raw}`
    );
  }
  // Datatype property — a typed scalar carrying its datatype URI.
  return { lit: exactLexical(prop, raw), datatype: prop.datatype! };
}

/**
 * Canonicalize an embedded value: a map with no `$id`, an optional `$name` (the
 * authored dict-key — hash-relevant), an optional `$type`, and schema-mapped
 * fields. An explicit `$type` emits a type statement inside the embedded (and is
 * therefore hash-relevant even when it equals the range-derived type); without
 * it, fields are mapped via the containing property's `range` and NO type
 * statement is emitted — range-derived typing is inference only.
 */
function embeddedValue(
  prop: CodecProp,
  map: Record<string, unknown>,
  schema: CodecSchema
): CanonicalInputValue {
  if ('$id' in map) {
    throw new Error(
      `An embedded value under ${prop.predicate} must not carry $id — ` +
        'to point at a named resource, pass a reference ({ $ref }).'
    );
  }
  const types = validatedTypes(map, `Embedded value under ${prop.predicate}`);
  const explicitType = typeof map.$type === 'string' ? map.$type : undefined;
  const clsUri = explicitType ?? prop.range;
  if (!clsUri) {
    throw new Error(
      `Cannot map embedded value under ${prop.predicate}: it carries no $type ` +
        'and the property declares no range.'
    );
  }
  const cls = hashClassFor(schema, clsUri, 'embedded type');

  const statements = fieldStatements(map, cls, schema);
  if (types) {
    // A multi-typed embedded ($types implies an explicit $type): one type
    // statement per member, in $types (UTF-8 sorted) order — all hash-relevant.
    for (const member of types) {
      statements.push({ predicate: schema.typePredicate, value: { ref: member } });
    }
  } else if (explicitType) {
    statements.push({ predicate: schema.typePredicate, value: { ref: explicitType } });
  }
  const name = typeof map.$name === 'string' && map.$name.length > 0 ? map.$name : undefined;
  return name !== undefined ? { embed: { name, statements } } : { embed: { statements } };
}

/**
 * The statements for one node-or-embedded's modeled fields + its `$extra`
 * (everything except the type triple, which subjects always carry and embeddeds
 * carry only when explicitly typed).
 */
function fieldStatements(
  source: Record<string, unknown>,
  cls: CodecClass,
  schema: CodecSchema
): CanonicalInputStatement[] {
  const statements: CanonicalInputStatement[] = [];

  for (const [key, raw] of Object.entries(source)) {
    if (ENVELOPE_KEYS.has(key)) continue;
    if (raw === undefined || raw === null) continue;
    const prop = cls.props[key];
    if (!prop) throw undeclaredStatement(cls, `"${key}"`);
    if (Array.isArray(raw)) {
      // An empty list contributes NO statement — absent and empty are identical
      // at the canonical layer (the wire serialize still preserves the empty list).
      if (raw.length === 0) continue;
      statements.push({
        predicate: prop.predicate,
        value: { list: raw.map((item) => valueOf(prop, item, schema)) },
      });
    } else {
      statements.push({ predicate: prop.predicate, value: valueOf(prop, raw, schema) });
    }
  }

  // Open-world extras ride through serialize and deserialize, but this codec
  // can't type them, so it refuses to hash them rather than hash them as text.
  const extra = source.$extra;
  if (extra && typeof extra === 'object') {
    for (const [pred, raw] of Object.entries(extra as Record<string, unknown>)) {
      if (raw === undefined || raw === null) continue;
      throw undeclaredStatement(cls, `$extra statement ${pred}`);
    }
  }

  return statements;
}

/**
 * A codec holds one package's schema, so it can't know the range of a
 * predicate another package declares on its classes (open-world augmentation).
 * Hashing such a statement as untyped text would give a hash that differs from
 * `kanonak hash`'s without a word, so hashing refuses it, naming it
 * (`[hash-undeclared-statement]`). Serializing and deserializing carry it.
 */
function undeclaredStatement(cls: CodecClass, what: string): Error {
  return new Error(
    `Cannot hash ${cls.typeUri}'s ${what}: this codec's schema doesn't declare it, so it can't type its value. ` +
      'Hash it with the SDK, over the graph [hash-undeclared-statement]',
  );
}

/** Build the statements for one subject node: its type triple(s) + its fields. */
function statementsFor(node: CodecNode, schema: CodecSchema): CanonicalInputStatement[] {
  const types = validatedTypes(node, `Node ${node.$id ?? '(no $id)'}`);
  assertReferenceValues(node, `Node ${node.$id ?? '(no $id)'}`);
  const typeUri = node.$type;
  if (!typeUri) throw new Error(`Node ${node.$id ?? '(no $id)'} is missing $type`);
  const cls = hashClassFor(schema, typeUri, 'type');

  return [
    // The rdf:type triple(s) every resource carries: one per $types member for
    // a multi-typed node (in $types' UTF-8 sorted order), else the single $type.
    ...(types ?? [typeUri]).map((member): CanonicalInputStatement => ({
      predicate: schema.typePredicate,
      value: { ref: member },
    })),
    ...fieldStatements(node as unknown as Record<string, unknown>, cls, schema),
  ];
}

/**
 * Build the `CanonicalInput` for a set of resource nodes plus the synthesized
 * package-wrapper subject — exactly the subject set `kanonak hash` produces for
 * the equivalent authored package. Statement/subject ordering is irrelevant
 * (the canonical entry orders by predicate/URI UTF-8 bytes).
 */
export function buildCanonicalInput(
  nodes: CodecNode[],
  schema: CodecSchema,
  pkg: PackageContext
): CanonicalInput {
  const subjects = nodes.map((node) => {
    if (!node.$id) throw new Error(`Node of type ${node.$type} is missing $id`);
    return { uri: node.$id, statements: statementsFor(node, schema) };
  });

  // The package-wrapper subject: `<publisher>/<package>@<version>/<package>`
  // with a raw label (the parser emits the package label untyped) + its type triple.
  const pkgUri = `${pkg.publisher}/${pkg.packageName}@${pkg.version}/${pkg.packageName}`;
  const pkgStatements: CanonicalInputStatement[] = [];
  if (pkg.label !== undefined) {
    pkgStatements.push({ predicate: schema.labelPredicate, value: { raw: pkg.label } });
  }
  pkgStatements.push({ predicate: schema.typePredicate, value: { ref: schema.packageTypeUri } });
  subjects.push({ uri: pkgUri, statements: pkgStatements });

  return { subjects };
}

/** The canonical form (the `{subjects:[…]}` JSON) of a package built from nodes. */
export function packageCanonicalForm(
  nodes: CodecNode[],
  schema: CodecSchema,
  pkg: PackageContext
): string {
  return canonicalForm(buildCanonicalInput(nodes, schema, pkg));
}

/** The `sha256:` content hash of a package built from nodes — matches `kanonak hash`. */
export function packageContentHash(
  nodes: CodecNode[],
  schema: CodecSchema,
  pkg: PackageContext
): string {
  return canonicalHash(buildCanonicalInput(nodes, schema, pkg));
}

/**
 * Serialize a typed node to its normalized-JSON wire form. The node is already
 * the wire shape (`$`-envelope + alias-collapsed local-name fields + `Ref<T>`
 * values), so this is a shallow projection that (a) drops `undefined`, and (b)
 * spreads `$extra` entries back to TOP LEVEL — the open-world assertions ride
 * as sibling fields on the wire (`[JsonExtensionData]` semantics), not under a
 * `$extra` key. A modeled field always wins a name collision with an extra.
 */
/** @deprecated Use {@link serializeExact}, which writes integer and decimal values in their JSON forms (runtime#54). */
export function serialize(node: CodecNode): Record<string, unknown> {
  // Producer-side $types validation, at every depth — fail closest to the bug.
  assertTypesEnvelopes(node, `serialize ${node.$id ?? node.$type ?? '(node)'}`);
  assertReferenceValues(node, `serialize ${node.$id ?? node.$type ?? '(node)'}`);
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(node)) {
    if (k === '$extra') continue;
    if (v !== undefined) out[k] = v;
  }
  if (node.$extra) {
    for (const [k, v] of Object.entries(node.$extra)) {
      if (v !== undefined && !(k in out)) out[k] = v;
    }
  }
  return out;
}

/**
 * Parse normalized-JSON into a typed node. `$`-envelope keys and the fields
 * modeled on the node's `$type` stay top-level; every other key is an
 * open-world assertion collected into `$extra` so a strongly-typed consumer
 * round-trips it losslessly. Requires `$type` (the one field that cannot be
 * inferred) and a class in the schema that can read it: the exact class, or
 * the same class at a compatible later version (runtime#28 — see
 * {@link classFor}). The node keeps the `$type` it was WRITTEN with; the
 * producer's bytes are the producer's.
 */
/** @deprecated Use {@link deserializeExact}, which reads integer and decimal values exactly (runtime#54). */
export function deserialize(json: Record<string, unknown>, schema: CodecSchema): CodecNode {
  const typeUri = json.$type;
  if (typeof typeUri !== 'string') throw new Error('Cannot deserialize: missing string $type');
  // Reader-side $types validation, at every depth: an unsorted / singleton /
  // duplicate / non-member set is REJECTED, never silently repaired —
  // determinism belongs to the producer, and a lenient reader would mask a
  // nondeterministic emitter.
  assertTypesEnvelopes(json, `deserialize ${typeof json.$id === 'string' ? json.$id : typeUri}`);
  assertReferenceValues(json, `deserialize ${typeof json.$id === 'string' ? json.$id : typeUri}`);
  const match = classFor(schema, typeUri);
  if (!('cls' in match)) throw new Error(`Cannot deserialize: ${match.error}`);
  const cls = match.cls;

  const node: CodecNode = { $type: typeUri };
  const fields = node as Record<string, unknown>;
  let extra: Record<string, unknown> | undefined;
  for (const [k, v] of Object.entries(json)) {
    if (k === '$type') continue;
    if (k.startsWith('$') || k in cls.props) {
      fields[k] = v;
    } else {
      (extra ??= {})[k] = v;
    }
  }
  if (extra) node.$extra = extra;
  return node;
}

// ---------------------------------------------------------------------------
// The JSON forms of integer and decimal values (kanonak-protocol 4.11.0's
// `numbers-in-json`, runtime#54). A JSON number can't carry every integer or
// decimal exactly in every language, so a wide integer and a decimal travel as
// a string of their digits, and a reader refuses a number that may already have
// been rounded.
// ---------------------------------------------------------------------------

const XSD = 'kanonak.org/core-xsd';

/**
 * How JSON carries a datatype's values: a wide integer as a string, read from a string or a whole number within
 * ±(2^53 − 1); a decimal as a string only; a narrow integer as a number only. Each inside its datatype's range.
 */
type JsonNumberForm =
  | { readonly form: 'wide'; readonly min?: bigint; readonly max?: bigint }
  | { readonly form: 'decimal' }
  | { readonly form: 'narrow'; readonly min: bigint; readonly max: bigint };

const JSON_NUMBER_FORMS: ReadonlyMap<string, JsonNumberForm> = new Map<string, JsonNumberForm>([
  [`${XSD}/integer`, { form: 'wide' }],
  [`${XSD}/nonNegativeInteger`, { form: 'wide', min: 0n }],
  [`${XSD}/positiveInteger`, { form: 'wide', min: 1n }],
  [`${XSD}/nonPositiveInteger`, { form: 'wide', max: 0n }],
  [`${XSD}/negativeInteger`, { form: 'wide', max: -1n }],
  [`${XSD}/long`, { form: 'wide', min: -(2n ** 63n), max: 2n ** 63n - 1n }],
  [`${XSD}/unsignedLong`, { form: 'wide', min: 0n, max: 2n ** 64n - 1n }],
  [`${XSD}/decimal`, { form: 'decimal' }],
  [`${XSD}/int`, { form: 'narrow', min: -(2n ** 31n), max: 2n ** 31n - 1n }],
  [`${XSD}/short`, { form: 'narrow', min: -(2n ** 15n), max: 2n ** 15n - 1n }],
  [`${XSD}/byte`, { form: 'narrow', min: -(2n ** 7n), max: 2n ** 7n - 1n }],
  [`${XSD}/unsignedInt`, { form: 'narrow', min: 0n, max: 2n ** 32n - 1n }],
  [`${XSD}/unsignedShort`, { form: 'narrow', min: 0n, max: 2n ** 16n - 1n }],
  [`${XSD}/unsignedByte`, { form: 'narrow', min: 0n, max: 2n ** 8n - 1n }],
]);

type NumberConversion = (prop: CodecProp, form: JsonNumberForm, value: unknown) => unknown;

function jsonNumberError(prop: CodecProp, why: string): Error {
  return new Error(`${prop.predicate}: ${why} [json-number-form]`);
}

function describeJson(value: unknown): string {
  return typeof value === 'string' ? `the string ${JSON.stringify(value)}` : `${typeof value} ${String(value)}`;
}

function inRange(prop: CodecProp, form: JsonNumberForm, n: bigint): bigint {
  const { min, max } = form as { min?: bigint; max?: bigint };
  if ((min !== undefined && n < min) || (max !== undefined && n > max)) {
    throw jsonNumberError(prop, `${n} is outside ${prop.datatype}'s range`);
  }
  return n;
}

function integerOfLexical(prop: CodecProp, lexical: string): bigint {
  try {
    return BigInt(canonicalInteger(lexical));
  } catch {
    throw jsonNumberError(prop, `${JSON.stringify(lexical)} is not an integer lexical form`);
  }
}

function decimalOfLexical(prop: CodecProp, lexical: string): string {
  try {
    return canonicalDecimal(lexical);
  } catch {
    throw jsonNumberError(prop, `${JSON.stringify(lexical)} is not a decimal lexical form`);
  }
}

/** A value as JSON carries it, to its exact type here: a wide integer a `bigint`, a decimal its canonical digits, a narrow integer a `number`. */
const readJsonNumber: NumberConversion = (prop, form, value) => {
  switch (form.form) {
    case 'decimal':
      if (typeof value !== 'string') {
        throw jsonNumberError(prop, `a decimal is read from a JSON string only, so it can't have been rounded; got ${describeJson(value)}`);
      }
      return decimalOfLexical(prop, value);
    case 'wide':
      if (typeof value === 'string') return inRange(prop, form, integerOfLexical(prop, value));
      if (typeof value === 'number' && Number.isSafeInteger(value)) return inRange(prop, form, BigInt(value));
      throw jsonNumberError(
        prop,
        `a wide integer is read from a string, or from a whole JSON number within ±(2^53 − 1); got ${describeJson(value)}`,
      );
    case 'narrow':
      if (typeof value !== 'number' || !Number.isInteger(value)) {
        throw jsonNumberError(prop, `a narrow integer is read from a whole JSON number only; got ${describeJson(value)}`);
      }
      inRange(prop, form, BigInt(value));
      return value;
  }
};

/** A value of its exact type here, to its JSON form: a wide integer and a decimal as a string of canonical digits, a narrow integer as a number. */
const writeJsonNumber: NumberConversion = (prop, form, value) => {
  switch (form.form) {
    case 'decimal':
      if (typeof value !== 'string') {
        throw jsonNumberError(prop, `pass a decimal as a string of its digits, which holds it exactly; got ${describeJson(value)}`);
      }
      return decimalOfLexical(prop, value);
    case 'wide': {
      const n =
        typeof value === 'bigint' ? value
          : typeof value === 'string' ? integerOfLexical(prop, value)
            : typeof value === 'number' && Number.isSafeInteger(value) ? BigInt(value)
              : undefined;
      if (n === undefined) {
        throw jsonNumberError(prop, `pass a wide integer as a bigint, a string, or a whole number within ±(2^53 − 1); got ${describeJson(value)}`);
      }
      return inRange(prop, form, n).toString();
    }
    case 'narrow': {
      const n = typeof value === 'bigint' ? value : typeof value === 'number' && Number.isInteger(value) ? BigInt(value) : undefined;
      if (n === undefined) throw jsonNumberError(prop, `pass a narrow integer as a whole number; got ${describeJson(value)}`);
      return Number(inRange(prop, form, n));
    }
  }
};

function jsonNumberFormOf(prop: CodecProp): JsonNumberForm | undefined {
  if (prop.kind !== 'datatype' || !prop.datatype) return undefined;
  const key = lenientVersionlessKey(prop.datatype);
  return key === undefined ? undefined : JSON_NUMBER_FORMS.get(key);
}

function classOrThrow(schema: CodecSchema, typeUri: string, what: string): CodecClass {
  const match = classFor(schema, typeUri);
  if (!('cls' in match)) throw new Error(`Cannot convert ${what}: ${match.error}`);
  return match.cls;
}

/** Each modeled field's value, through lists and embedded values, with its integers and decimals converted. */
function convertFields(
  fields: Record<string, unknown>,
  cls: CodecClass,
  schema: CodecSchema,
  convert: NumberConversion,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...fields };
  for (const [name, prop] of Object.entries(cls.props)) {
    const value = out[name];
    if (value !== undefined && value !== null) out[name] = convertValue(prop, value, schema, convert);
  }
  return out;
}

function convertValue(prop: CodecProp, value: unknown, schema: CodecSchema, convert: NumberConversion): unknown {
  if (Array.isArray(value)) return value.map((item) => convertValue(prop, item, schema, convert));
  if (prop.kind === 'datatype') {
    const form = jsonNumberFormOf(prop);
    return form ? convert(prop, form, value) : value;
  }
  if (value && typeof value === 'object' && !('$ref' in value)) {
    const map = value as Record<string, unknown>;
    const clsUri = typeof map.$type === 'string' ? map.$type : prop.range;
    if (!clsUri) {
      throw new Error(`Cannot convert the embedded value under ${prop.predicate}: it carries no $type and the property declares no range.`);
    }
    return convertFields(map, classOrThrow(schema, clsUri, `the embedded value under ${prop.predicate}`), schema, convert);
  }
  return value;
}

/**
 * Serialize a node to normalized JSON, writing each integer and decimal value in its JSON form (kanonak-protocol
 * 4.11.0's `numbers-in-json`, runtime#54): a wide integer and a decimal as a string of their canonical digits, a
 * narrow integer as a number. A value outside its datatype's range is refused, ending `[json-number-form]`. The
 * result holds no `bigint`, so `JSON.stringify` writes it as it stands.
 */
export function serializeExact(node: CodecNode, schema: CodecSchema): Record<string, unknown> {
  const typeUri = node.$type;
  if (typeof typeUri !== 'string') throw new Error('Cannot serialize: missing string $type');
  const cls = classOrThrow(schema, typeUri, `node ${node.$id ?? typeUri}`);
  return serialize(convertFields(node as Record<string, unknown>, cls, schema, writeJsonNumber) as CodecNode);
}

/**
 * Parse normalized JSON into a node, as {@link deserialize} does, reading each integer and decimal value exactly: a
 * wide integer to a `bigint`, from a string or a whole number within ±(2^53 − 1); a decimal to its canonical digits,
 * from a string only; a narrow integer to a `number`, from a whole number only. A value JSON may already have rounded,
 * or one outside its datatype's range, is refused, ending `[json-number-form]`.
 */
export function deserializeExact(json: Record<string, unknown>, schema: CodecSchema): CodecNode {
  const node = deserialize(json, schema);
  const cls = classOrThrow(schema, node.$type!, `node ${node.$id ?? node.$type}`);
  return convertFields(node as Record<string, unknown>, cls, schema, readJsonNumber) as CodecNode;
}
