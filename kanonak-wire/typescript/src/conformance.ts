/**
 * Drives the shared wire vectors through this port. Read vectors run an
 * op-script against a hex buffer asserting values or required errors
 * {kind, offset}; write vectors run writer ops asserting exact output bytes.
 * Run: `npm run conformance`.
 */
import { readFileSync } from 'node:fs';
import { WireError, WireReader, WireWriter } from './index.js';

// TypeScript has dynamic numbers and UTF-16 strings: all capabilities present.
const CAPABILITIES = new Set(['wide-numeric-params', 'dynamic-numeric', 'utf16-strings']);

interface ExpectError {
  kind: string;
  offset?: number;
}

interface ReadOp {
  op: string;
  n?: number;
  expected?: number | string;
  expectError?: ExpectError;
}

interface WriteOp {
  op: string;
  value?: number | string;
  hex?: string;
  utf16CodeUnits?: number[];
  expectError?: ExpectError;
}

interface ReadVector {
  id: string;
  bytes: string;
  ops: ReadOp[];
  requires?: string;
}

interface WriteVector {
  id: string;
  ops: WriteOp[];
  expectedBytes?: string;
  requires?: string;
}

const vfile = new URL('../../vectors/wire-vectors.json', import.meta.url);
const data = JSON.parse(readFileSync(vfile, 'utf8')) as {
  readVectors: ReadVector[];
  writeVectors: WriteVector[];
};

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function bytesToHex(b: Uint8Array): string {
  let h = '';
  for (const x of b) h += x.toString(16).padStart(2, '0');
  return h;
}

let pass = 0;
let fail = 0;
let skipped = 0;

function failCase(id: string, msg: string): void {
  fail++;
  console.error(`${id}: ${msg}`);
}

function checkError(id: string, opName: string, e: unknown, want: ExpectError): boolean {
  if (!(e instanceof WireError)) {
    failCase(id, `${opName}: threw a non-WireError: ${(e as Error).message}`);
    return false;
  }
  if (e.kind !== want.kind) {
    failCase(id, `${opName}: expected error kind ${want.kind}, got ${e.kind} (${e.message})`);
    return false;
  }
  if (want.offset !== undefined && e.offset !== want.offset) {
    failCase(id, `${opName}: expected error offset ${want.offset}, got ${e.offset}`);
    return false;
  }
  return true;
}

function runReadOp(r: WireReader, op: ReadOp): number | string | undefined {
  switch (op.op) {
    case 'u8': return r.u8();
    case 'u16be': return r.u16be();
    case 'u32be': return r.u32be();
    case 'bytes': return bytesToHex(r.bytes(op.n as number));
    case 'uuid': return r.uuid();
    case 'utf8': return r.utf8(op.n as number);
    case 'lenPrefixedBytes16': return bytesToHex(r.lenPrefixedBytes16());
    case 'rest': return bytesToHex(r.rest());
    case 'remaining': return r.remaining();
    case 'expectEnd': r.expectEnd(); return undefined;
    default: throw new Error(`conformance: unknown read op '${op.op}'`);
  }
}

for (const v of data.readVectors) {
  if (v.requires && !CAPABILITIES.has(v.requires)) { skipped++; continue; }
  const r = new WireReader(hexToBytes(v.bytes));
  let ok = true;
  for (const op of v.ops) {
    if (op.expectError) {
      try {
        runReadOp(r, op);
        failCase(v.id, `${op.op}: expected ${op.expectError.kind}, got a value`);
        ok = false;
      } catch (e) {
        if (!checkError(v.id, op.op, e, op.expectError)) ok = false;
      }
      break; // an error op ends the script
    }
    let got: number | string | undefined;
    try {
      got = runReadOp(r, op);
    } catch (e) {
      failCase(v.id, `${op.op}: threw ${(e as Error).message}`);
      ok = false;
      break;
    }
    if (op.expected !== undefined && got !== op.expected) {
      failCase(v.id, `${op.op}: expected ${JSON.stringify(op.expected)}, got ${JSON.stringify(got)}`);
      ok = false;
      break;
    }
  }
  if (ok) pass++;
}

function runWriteOp(w: WireWriter, op: WriteOp): void {
  switch (op.op) {
    case 'u8': w.u8(op.value as number); return;
    case 'u16be': w.u16be(op.value as number); return;
    case 'u32be': w.u32be(op.value as number); return;
    case 'bytes': w.bytes(hexToBytes(op.hex as string)); return;
    case 'uuid': w.uuid(op.value as string); return;
    case 'utf8': {
      const s = op.utf16CodeUnits
        ? String.fromCharCode(...op.utf16CodeUnits)
        : (op.value as string);
      w.utf8(s);
      return;
    }
    case 'lenPrefixedBytes16': w.lenPrefixedBytes16(hexToBytes(op.hex as string)); return;
    default: throw new Error(`conformance: unknown write op '${op.op}'`);
  }
}

for (const v of data.writeVectors) {
  if (v.requires && !CAPABILITIES.has(v.requires)) { skipped++; continue; }
  const w = new WireWriter();
  let ok = true;
  for (const op of v.ops) {
    if (op.expectError) {
      try {
        runWriteOp(w, op);
        failCase(v.id, `${op.op}: expected ${op.expectError.kind}, got success`);
        ok = false;
      } catch (e) {
        if (!checkError(v.id, op.op, e, op.expectError)) ok = false;
      }
      break;
    }
    try {
      runWriteOp(w, op);
    } catch (e) {
      failCase(v.id, `${op.op}: threw ${(e as Error).message}`);
      ok = false;
      break;
    }
  }
  if (ok && v.expectedBytes !== undefined) {
    const got = bytesToHex(w.toBytes());
    if (got !== v.expectedBytes) {
      failCase(v.id, `expected bytes ${v.expectedBytes}, got ${got}`);
      ok = false;
    }
  }
  if (ok) pass++;
}

const total = data.readVectors.length + data.writeVectors.length;
console.log(`wire-vectors: ${pass}/${total} pass (${skipped} skipped)`);
if (fail > 0) {
  console.error(`${fail} VECTOR(S) FAILED`);
  process.exit(1);
}
console.log('ALL VECTORS PASS');
