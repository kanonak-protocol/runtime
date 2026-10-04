// Drives the shared codec golden vectors through the WebAssembly component's
// WIT surface — the same files and the same checks as the six native ports
// (mirrors rust/tests/conformance.rs). The component is transpiled by jco
// first (see package.json); this host then calls the four exported functions
// over the JSON-string ABI.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

function readDoc(file) {
  return JSON.parse(
    readFileSync(join(here, "..", "..", "vectors", file), "utf8"),
  );
}

const mod = await import("./dist/kanonak_codec_wasm.js");
// jco names the export after the interface and its package version, which
// tracks the codec version (#23), so match the interface, not one version.
const codec =
  mod.codec ??
  Object.entries(mod).find(([name]) => name.startsWith("kanonak:codec/codec@"))?.[1];
if (!codec) {
  throw new Error(
    "component export kanonak:codec/codec not found; module exports: " +
      Object.keys(mod).join(", "),
  );
}

// The ABI is JSON text in / JSON text out; a WIT `result` error surfaces in a
// jco host as a thrown exception carrying the error string.
function tryCall(fn, ...args) {
  try {
    return { ok: codec[fn](...args.map((a) => JSON.stringify(a))) };
  } catch (e) {
    const err = e?.payload ?? e?.message ?? String(e);
    return { err: typeof err === "string" ? err : JSON.stringify(err) };
  }
}

function deepEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((v, i) => deepEqual(v, b[i]));
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    return (
      ka.length === kb.length && ka.every((k) => k in b && deepEqual(a[k], b[k]))
    );
  }
  return false;
}

let fails = 0;
function fail(msg) {
  fails += 1;
  console.error("FAIL " + msg);
}

function runFile(file) {
  const doc = readDoc(file);
  for (const c of doc.cases) {
    const form = tryCall("canonicalForm", c.nodes, doc.schema, c.pkg);
    if (form.err !== undefined) {
      fail(`[${c.id}] canonical-form rejected: ${form.err}`);
    } else if (form.ok !== c.expectedCanonicalForm) {
      fail(
        `[${c.id}] form\n  expected: ${c.expectedCanonicalForm}\n  actual:   ${form.ok}`,
      );
    }

    const hash = tryCall("contentHash", c.nodes, doc.schema, c.pkg);
    if (hash.err !== undefined) {
      fail(`[${c.id}] content-hash rejected: ${hash.err}`);
    } else if (hash.ok !== c.expectedHash) {
      fail(`[${c.id}] hash expected ${c.expectedHash} got ${hash.ok}`);
    }

    c.nodes.forEach((node, i) => {
      const wire = tryCall("serialize", node);
      if (wire.err !== undefined) {
        fail(`[${c.id}] serialize[${i}] rejected: ${wire.err}`);
      } else if (!deepEqual(JSON.parse(wire.ok), c.expectedSerialize[i])) {
        fail(
          `[${c.id}] serialize[${i}]\n  expected: ${JSON.stringify(c.expectedSerialize[i])}\n  actual:   ${wire.ok}`,
        );
      }
    });
  }
}

// The 0.4.0 multi-typed-subjects file: expectError cases must be rejected on
// ALL THREE surfaces (serialize / deserialize / canonicalization), and
// positive cases must round-trip — deserialize(serialize(x)) preserves $types
// exactly and re-canonicalizes to the same hash.
function runTypesFile(file) {
  const doc = readDoc(file);
  for (const c of doc.cases) {
    if (c.expectError === true) {
      if (tryCall("canonicalForm", c.nodes, doc.schema, c.pkg).err === undefined) {
        fail(`[${c.id}] expected canonicalize to reject, it did not`);
      }
      if (c.nodes.every((n) => tryCall("serialize", n).err === undefined)) {
        fail(`[${c.id}] expected serialize to reject, it did not`);
      }
      if (c.nodes.every((n) => tryCall("deserialize", n, doc.schema).err === undefined)) {
        fail(`[${c.id}] expected deserialize to reject, it did not`);
      }
      continue;
    }

    const form = tryCall("canonicalForm", c.nodes, doc.schema, c.pkg);
    if (form.err !== undefined) {
      fail(`[${c.id}] canonical-form rejected: ${form.err}`);
    } else if (form.ok !== c.expectedCanonicalForm) {
      fail(
        `[${c.id}] form\n  expected: ${c.expectedCanonicalForm}\n  actual:   ${form.ok}`,
      );
    }

    const hash = tryCall("contentHash", c.nodes, doc.schema, c.pkg);
    if (hash.err !== undefined) {
      fail(`[${c.id}] content-hash rejected: ${hash.err}`);
    } else if (hash.ok !== c.expectedHash) {
      fail(`[${c.id}] hash expected ${c.expectedHash} got ${hash.ok}`);
    }

    const roundTripped = [];
    c.nodes.forEach((node, i) => {
      const wire = tryCall("serialize", node);
      if (wire.err !== undefined) {
        fail(`[${c.id}] serialize[${i}] rejected: ${wire.err}`);
        return;
      }
      const wireObj = JSON.parse(wire.ok);
      if (!deepEqual(wireObj, c.expectedSerialize[i])) {
        fail(
          `[${c.id}] serialize[${i}]\n  expected: ${JSON.stringify(c.expectedSerialize[i])}\n  actual:   ${wire.ok}`,
        );
      }
      const back = tryCall("deserialize", wireObj, doc.schema);
      if (back.err !== undefined) {
        fail(`[${c.id}] round-trip deserialize[${i}] rejected: ${back.err}`);
        return;
      }
      const backObj = JSON.parse(back.ok);
      const reWire = tryCall("serialize", backObj);
      if (
        reWire.err !== undefined ||
        !deepEqual(JSON.parse(reWire.ok), c.expectedSerialize[i])
      ) {
        fail(`[${c.id}] round-trip serialize[${i}] mismatch`);
      }
      roundTripped.push(backObj);
    });

    if (roundTripped.length === c.nodes.length) {
      const rt = tryCall("contentHash", roundTripped, doc.schema, c.pkg);
      if (rt.err !== undefined || rt.ok !== c.expectedHash) {
        fail(
          `[${c.id}] round-trip hash expected ${c.expectedHash} got ${rt.err ?? rt.ok}`,
        );
      }
    }
  }
}

// The compatibility file (runtime#28) through the WIT surface: the deserialize
// and hash sections. typeMatches / enumMember are library helpers for
// generated code, not part of the component's interface.
function runCompatFile(file) {
  const doc = readDoc(file);
  const rejected = (r, kind, id, what) => {
    if (r.err === undefined) {
      fail(`[${id}] ${what} expected a [${kind}] rejection, got ${r.ok}`);
    } else if (!r.err.endsWith(`[${kind}]`)) {
      fail(`[${id}] ${what} rejected, but not as [${kind}]: ${r.err}`);
    }
  };
  for (const c of doc.deserializeCases) {
    const r = tryCall("deserialize", c.input, doc.schemas[c.schema]);
    if (c.expectError) rejected(r, c.expectError, c.id, "deserialize");
    else if (r.err !== undefined) fail(`[${c.id}] deserialize errored: ${r.err}`);
    else if (!deepEqual(JSON.parse(r.ok), c.expected)) {
      fail(`[${c.id}] deserialize mismatch: ${r.ok}`);
    }
  }
  for (const c of doc.hashCases) {
    const r = tryCall("contentHash", c.nodes, doc.schemas[c.schema], doc.pkg);
    if (c.expectError) rejected(r, c.expectError, c.id, "contentHash");
    else if (r.err !== undefined || r.ok !== c.expectedHash) {
      fail(`[${c.id}] hash expected ${c.expectedHash} got ${r.err ?? r.ok}`);
    }
  }
}

runFile("codec-vectors.json");
runFile("codec-vectors-embedded.json");
runTypesFile("codec-vectors-types.json");
runCompatFile("codec-vectors-compat.json");

if (fails > 0) {
  console.error(`kanonak-codec wasm component: ${fails} vector check(s) failed`);
  process.exit(1);
}
console.log("kanonak-codec wasm component: all codec vector checks passed");
