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
import { canonicalForm, canonicalHash } from '@kanonak-protocol/canonical';
import type { CanonicalInput, CanonicalInputStatement, CanonicalInputValue } from '@kanonak-protocol/canonical';

/** Reserved `$`-envelope keys — never emitted as ontology statements. */
const ENVELOPE_KEYS = new Set(['$type', '$id', '$name', '$contentHash', '$version', '$extra']);

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

/** A class's canonicalization schema: its durable URI + its (flattened) props. */
export interface CodecClass {
  /** The class's durable canonical URI — the value of the synthesized type triple. */
  typeUri: string;
  /** Properties keyed by local name (the wire field name). */
  props: Record<string, CodecProp>;
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
  return typeof value === 'string' ? value : String(value);
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
  return { lit: lexical(raw), datatype: prop.datatype! };
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
  const explicitType = typeof map.$type === 'string' ? map.$type : undefined;
  const clsUri = explicitType ?? prop.range;
  if (!clsUri) {
    throw new Error(
      `Cannot map embedded value under ${prop.predicate}: it carries no $type ` +
        'and the property declares no range.'
    );
  }
  const cls = schema.classes[clsUri];
  if (!cls) throw new Error(`No schema for embedded type ${clsUri}`);

  const statements = fieldStatements(map, cls, schema);
  if (explicitType) {
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
    if (!prop) {
      // Not in the type-model — an open-world assertion. Preserved as a raw token.
      statements.push({ predicate: key, value: { raw: lexical(raw) } });
      continue;
    }
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

  // Open-world extras outside the type-model, keyed by their own predicate URI.
  const extra = source.$extra;
  if (extra && typeof extra === 'object') {
    for (const [pred, raw] of Object.entries(extra as Record<string, unknown>)) {
      if (raw === undefined || raw === null) continue;
      statements.push({ predicate: pred, value: { raw: lexical(raw) } });
    }
  }

  return statements;
}

/** Build the statements for one subject node: the type triple + its fields. */
function statementsFor(node: CodecNode, schema: CodecSchema): CanonicalInputStatement[] {
  const typeUri = node.$type;
  if (!typeUri) throw new Error(`Node ${node.$id ?? '(no $id)'} is missing $type`);
  const cls = schema.classes[typeUri];
  if (!cls) throw new Error(`No schema for type ${typeUri}`);

  return [
    // The rdf:type triple every resource carries.
    { predicate: schema.typePredicate, value: { ref: typeUri } },
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
export function serialize(node: CodecNode): Record<string, unknown> {
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
 * inferred) and its class in the schema.
 */
export function deserialize(json: Record<string, unknown>, schema: CodecSchema): CodecNode {
  const typeUri = json.$type;
  if (typeof typeUri !== 'string') throw new Error('Cannot deserialize: missing string $type');
  const cls = schema.classes[typeUri];
  if (!cls) throw new Error(`Cannot deserialize: no schema for type ${typeUri}`);

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
