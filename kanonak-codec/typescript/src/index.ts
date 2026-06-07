/**
 * `@kanonak-protocol/codec` — the generic, ontology-INDEPENDENT codec runtime.
 *
 * Given a `CodecSchema` (the per-package metadata a generated SDK embeds) and a
 * set of typed nodes, it builds the language-neutral `CanonicalInput` model and
 * content-addresses it via the SDK's `canonicalHash` (the input-model entry,
 * kanonak-protocol/typescript#56). The same logic backs every generated package
 * — only the embedded `CodecSchema` differs — so it lives here as one audited,
 * vector-checked dependency rather than re-inlined per package.
 *
 * Self-contained: carriers come from the schema's datatype URIs (no runtime
 * ontology resolution), and the resolved foundation URIs (`type`/`label`
 * predicates, the `Package` class) are embedded by the generator. The output
 * therefore byte-matches `kanonak hash` of the equivalent authored package,
 * including the synthesized `rdf:type` triples and the package-wrapper subject.
 *
 * This is the TypeScript port of the codec runtime; the C#/Rust/Go/Java/Python
 * ports reference their `kanonak-canonical` library for the same `canonicalHash`
 * entry. (TypeScript's canonical library IS `@kanonak-protocol/sdk`.)
 */
import { canonicalHash, canonicalForm, PackageBuilder } from '@kanonak-protocol/sdk';
import type { CanonicalInput, CanonicalInputStatement, CanonicalInputValue } from '@kanonak-protocol/sdk';
import { KanonakParser } from '@kanonak-protocol/sdk/parsing';
import { InMemoryKanonakDocumentRepository } from '@kanonak-protocol/sdk/repositories';

/** Reserved `$`-envelope keys — never emitted as ontology statements. */
const ENVELOPE_KEYS = new Set(['$type', '$id', '$contentHash', '$version', '$extra']);

/** One property's canonicalization metadata, as embedded by the generator. */
export interface CodecProp {
  /** The predicate's durable canonical URI (with resolved version). */
  predicate: string;
  /** Datatype vs object — decides typed-scalar vs reference/embedded. */
  kind: 'datatype' | 'object';
  /** The datatype's canonical URI (carrier source) — present for datatype props. */
  datatype?: string;
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
function valueOf(prop: CodecProp, raw: unknown): CanonicalInputValue {
  if (prop.kind === 'object') {
    // A node: a reference (`{ $ref }`) or an embedded resource (its own $type + fields).
    if (raw && typeof raw === 'object' && '$ref' in (raw as Record<string, unknown>)) {
      return { ref: String((raw as { $ref: unknown }).$ref) };
    }
    // Embedded node — recurse. Embedded statements carry the same schema lookup.
    throw new Error(
      'Embedded object values are not yet supported by the codec runtime; ' +
        'pass a reference ({ $ref }) for now.'
    );
  }
  // Datatype property — a typed scalar carrying its datatype URI.
  return { lit: lexical(raw), datatype: prop.datatype! };
}

/** Build the statements for one node's modeled fields + its `$extra`. */
function statementsFor(node: CodecNode, schema: CodecSchema): CanonicalInputStatement[] {
  const typeUri = node.$type;
  if (!typeUri) throw new Error(`Node ${node.$id ?? '(no $id)'} is missing $type`);
  const cls = schema.classes[typeUri];
  if (!cls) throw new Error(`No schema for type ${typeUri}`);

  const statements: CanonicalInputStatement[] = [
    // The rdf:type triple every resource carries.
    { predicate: schema.typePredicate, value: { ref: typeUri } },
  ];

  for (const [key, raw] of Object.entries(node)) {
    if (ENVELOPE_KEYS.has(key)) continue;
    if (raw === undefined || raw === null) continue;
    const prop = cls.props[key];
    if (!prop) {
      // Not in the type-model — an open-world assertion. Preserved as a raw token.
      statements.push({ predicate: key, value: { raw: lexical(raw) } });
      continue;
    }
    if (Array.isArray(raw)) {
      statements.push({
        predicate: prop.predicate,
        value: { list: raw.map((item) => valueOf(prop, item)) },
      });
    } else {
      statements.push({ predicate: prop.predicate, value: valueOf(prop, raw) });
    }
  }

  // Open-world extras outside the type-model, keyed by their own predicate URI.
  if (node.$extra) {
    for (const [pred, raw] of Object.entries(node.$extra)) {
      if (raw === undefined || raw === null) continue;
      statements.push({ predicate: pred, value: { raw: lexical(raw) } });
    }
  }

  return statements;
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
  // (the package's own subject is named after the package) with a raw label (the
  // parser emits the package label untyped) + its type triple.
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
  return (canonicalForm as (i: CanonicalInput) => string)(buildCanonicalInput(nodes, schema, pkg));
}

/** The `sha256:` content hash of a package built from nodes — matches `kanonak hash`. */
export function packageContentHash(
  nodes: CodecNode[],
  schema: CodecSchema,
  pkg: PackageContext
): string {
  return (canonicalHash as (i: CanonicalInput) => string)(buildCanonicalInput(nodes, schema, pkg));
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
  // Extras ride as sibling fields, after the modeled ones; a modeled field
  // already present wins a name collision (extras never clobber the type-model).
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

// ── YAML / Package wire ──────────────────────────────────────────────────────
// Emit typed nodes as a real `.kan.yml` Package (the agent-to-agent wire form)
// and parse one back. The body is assembled with `alias.name` references and
// dumped by the SDK's `PackageBuilder` (deterministic YAML, no repository
// resolution needed for emission). Identity stays the content hash — the
// emitted YAML's `kanonak hash` equals `packageContentHash`, but the YAML text
// itself is free-form presentation, not canonical.

interface ParsedUri {
  publisher: string;
  packageName: string;
  version: string;
  name: string;
  /** `publisher/package@version` — the resource's namespace. */
  namespace: string;
}

/** Split a durable URI `publisher/package@version/name` into its parts. */
function parseUri(uri: string): ParsedUri {
  const lastSlash = uri.lastIndexOf('/');
  if (lastSlash < 0) throw new Error(`Not a Kanonak URI: ${uri}`);
  const name = uri.slice(lastSlash + 1);
  const namespace = uri.slice(0, lastSlash);
  const firstSlash = namespace.indexOf('/');
  if (firstSlash < 0) throw new Error(`Malformed Kanonak URI: ${uri}`);
  const publisher = namespace.slice(0, firstSlash);
  const pkgAtVer = namespace.slice(firstSlash + 1);
  const at = pkgAtVer.indexOf('@');
  return {
    publisher,
    packageName: at >= 0 ? pkgAtVer.slice(0, at) : pkgAtVer,
    version: at >= 0 ? pkgAtVer.slice(at + 1) : '',
    name,
    namespace,
  };
}

/** Split an authored `alias.name` (or a bare local `name`) on its first dot. */
function splitAlias(s: string): { alias?: string; name: string } {
  const dot = s.indexOf('.');
  return dot < 0 ? { name: s } : { alias: s.slice(0, dot), name: s.slice(dot + 1) };
}

/** xsd datatypes whose values are JS numbers in the generated SDKs. */
const NUMBER_DATATYPES = new Set([
  'integer', 'int', 'long', 'short', 'byte', 'decimal', 'double', 'float',
  'nonnegativeinteger', 'nonpositiveinteger', 'positiveinteger', 'negativeinteger',
  'unsignedint', 'unsignedlong', 'unsignedshort', 'unsignedbyte',
]);

/**
 * Coerce a parsed scalar (the parser yields every body scalar as a string) back
 * to the JS type the generated SDK declares — numbers for the xsd numeric tree,
 * booleans for xsd:boolean, strings otherwise (incl. dates/times, which map to
 * `string` in every generator). A value that won't coerce is left as the string.
 */
function coerceScalar(datatypeUri: string | undefined, v: unknown): unknown {
  if (typeof v !== 'string' || !datatypeUri) return v;
  const dt = parseUri(datatypeUri).name.toLowerCase();
  if (NUMBER_DATATYPES.has(dt)) {
    const n = Number(v);
    return Number.isNaN(n) ? v : n;
  }
  if (dt === 'boolean') {
    if (v === 'true' || v === '1') return true;
    if (v === 'false' || v === '0') return false;
  }
  return v;
}

/**
 * Emit a set of typed nodes as a named `.kan.yml` Package. References resolve to
 * `alias.name` (cross-package) or a bare local name (same package); imports are
 * allocated for every referenced package. The emitted package's `kanonak hash`
 * equals `packageContentHash(nodes, schema, pkg)`.
 */
export async function toPackage(
  nodes: CodecNode[],
  schema: CodecSchema,
  pkg: PackageContext
): Promise<string> {
  const parser = new KanonakParser();
  const builder = new PackageBuilder(new InMemoryKanonakDocumentRepository(parser), parser);
  const book = builder.imports();
  const dataNs = `${pkg.publisher}/${pkg.packageName}@${pkg.version}`;

  const aliasFor = (uri: string): string => {
    const u = parseUri(uri);
    const preferred = u.packageName.split('.').pop() ?? 'i';
    return book.ensure(u.publisher, u.packageName, u.version, preferred);
  };
  const refToken = (refUri: string): string => {
    const r = parseUri(refUri);
    return r.namespace === dataNs ? r.name : `${aliasFor(refUri)}.${r.name}`;
  };
  const fieldValue = (prop: CodecProp, raw: unknown): unknown => {
    if (prop.kind === 'object') {
      if (raw && typeof raw === 'object' && '$ref' in (raw as Record<string, unknown>)) {
        return refToken(String((raw as { $ref: unknown }).$ref));
      }
      throw new Error('toPackage: embedded object values are not yet supported; pass a reference ({ $ref }).');
    }
    return raw; // datatype literal — PackageBuilder passes primitives/arrays through
  };

  const body: Record<string, unknown> = {};
  for (const node of nodes) {
    if (!node.$type) throw new Error('toPackage: node is missing $type');
    if (!node.$id) throw new Error('toPackage: node is missing $id');
    const cls = schema.classes[node.$type];
    if (!cls) throw new Error(`toPackage: no schema for type ${node.$type}`);

    const resource: Record<string, unknown> = {
      type: `${aliasFor(node.$type)}.${parseUri(node.$type).name}`,
    };
    for (const [key, raw] of Object.entries(node as Record<string, unknown>)) {
      if (ENVELOPE_KEYS.has(key) || raw === undefined || raw === null) continue;
      const prop = cls.props[key];
      if (!prop) continue; // unmodeled top-level keys live in $extra, handled below
      const k = `${aliasFor(prop.predicate)}.${parseUri(prop.predicate).name}`;
      resource[k] = Array.isArray(raw) ? raw.map((it) => fieldValue(prop, it)) : fieldValue(prop, raw);
    }
    if (node.$extra) {
      for (const [predUri, raw] of Object.entries(node.$extra)) {
        if (raw === undefined || raw === null) continue;
        resource[`${aliasFor(predUri)}.${parseUri(predUri).name}`] = raw;
      }
    }
    body[parseUri(node.$id).name] = resource;
  }

  const built = await builder.buildNamed({
    publisher: pkg.publisher,
    name: pkg.packageName,
    version: pkg.version,
    book,
    body,
    header: pkg.label !== undefined ? { label: pkg.label } : undefined,
  });
  return built.yaml;
}

/**
 * Parse a `.kan.yml` Package back into typed nodes. `alias.name` references
 * resolve through the document's own imports (no closure needed); a value's
 * reference-vs-literal interpretation comes from the schema's property kind.
 * Predicates not in the schema are preserved on `$extra` (keyed by durable URI).
 */
export function fromPackage(yaml: string, schema: CodecSchema): CodecNode[] {
  const parser = new KanonakParser();
  const doc = parser.parseWithErrors(yaml).document;
  if (!doc) throw new Error('fromPackage: failed to parse the package');
  const dataNs = doc.metadata.namespace_ ? doc.metadata.namespace_.toString() : '';

  const aliasNs = new Map<string, string>();
  for (const imp of doc.metadata.allImports ?? []) {
    if (imp.alias) aliasNs.set(imp.alias, `${imp.publisher}/${imp.packageName}@${imp.version}`);
  }
  const nsOf = (alias?: string): string | undefined => (alias ? aliasNs.get(alias) : dataNs);

  // predicate URI -> (local name, kind, datatype), across every class in the schema.
  const predToLocal = new Map<
    string,
    { localName: string; kind: 'datatype' | 'object'; datatype?: string }
  >();
  for (const cls of Object.values(schema.classes)) {
    for (const [localName, prop] of Object.entries(cls.props)) {
      predToLocal.set(prop.predicate, { localName, kind: prop.kind, datatype: prop.datatype });
    }
  }
  const toRef = (v: unknown): unknown => {
    if (typeof v !== 'string') return v;
    const { alias, name } = splitAlias(v);
    const ns = nsOf(alias);
    return ns ? { $ref: `${ns}/${name}` } : v;
  };

  const nodes: CodecNode[] = [];
  for (const [name, rawBody] of Object.entries(doc.body as Record<string, unknown>)) {
    const bodyObj = rawBody as Record<string, unknown>;
    if (typeof bodyObj.type !== 'string') continue;
    const t = splitAlias(bodyObj.type);
    const tns = nsOf(t.alias);
    if (!tns) continue;
    const typeUri = `${tns}/${t.name}`;
    if (!schema.classes[typeUri]) continue; // not one of this SDK's resource types

    const node: CodecNode = { $type: typeUri, $id: `${dataNs}/${name}` };
    const fields = node as Record<string, unknown>;
    let extra: Record<string, unknown> | undefined;
    for (const [k, v] of Object.entries(bodyObj)) {
      if (k === 'type') continue;
      const ks = splitAlias(k);
      const kns = nsOf(ks.alias);
      const predUri = kns ? `${kns}/${ks.name}` : k;
      const meta = predToLocal.get(predUri);
      if (meta) {
        fields[meta.localName] =
          meta.kind === 'object'
            ? Array.isArray(v) ? v.map(toRef) : toRef(v)
            : Array.isArray(v) ? v.map((x) => coerceScalar(meta.datatype, x)) : coerceScalar(meta.datatype, v);
      } else {
        (extra ??= {})[predUri] = v;
      }
    }
    if (extra) node.$extra = extra;
    nodes.push(node);
  }
  return nodes;
}
