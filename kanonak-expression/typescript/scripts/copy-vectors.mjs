// Ship the shared conformance vectors inside the npm package, so a host engine
// (the SDK's) can test itself against exactly the vectors of the runtime
// version it depends on. They live once, in ../../vectors; this copies them in
// at pack time (the copy is git-ignored).
import { cpSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../../vectors/', import.meta.url));
const target = fileURLToPath(new URL('../vectors/', import.meta.url));
rmSync(target, { recursive: true, force: true });
cpSync(source, target, { recursive: true });
