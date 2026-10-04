// The TypeScript half of the 1.x compatibility gate (runtime#29). TypeScript
// has no standard semver checker, so this asks the compiler directly:
//
//   node .github/scripts/ts_api_compat.mjs <member> <baseline-version>
//
// It unpacks the published @kanonak-protocol/<member>@<baseline> declarations
// and compares them with the candidate's (run the member's `npm run build`
// first, so dist/ holds them), by type-checking a generated file that asserts,
// for every export of the baseline:
//   - a value (function, const, class): the candidate's is assignable to the
//     baseline's type, so code compiled against the baseline still compiles;
//   - a type (interface, alias, class instance): baseline and candidate are
//     assignable to each other, so neither a producer nor a consumer of it
//     breaks. Adding an optional member passes; adding a required one,
//     removing one, or changing one fails.
// A missing export fails too. Exit 1 lists the compiler's diagnostics.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import ts from 'typescript';

const [member, baseline] = process.argv.slice(2);
if (!member || !baseline) {
  console.error('usage: ts_api_compat.mjs <member> <baseline-version>');
  process.exit(2);
}

const root = resolve(import.meta.dirname, '..', '..');
const dist = join(root, `kanonak-${member}`, 'typescript', 'dist');
if (!existsSync(join(dist, 'index.d.ts'))) {
  console.error(`${dist}/index.d.ts is missing; run the member's build first`);
  process.exit(2);
}

const work = mkdtempSync(join(tmpdir(), `ts-api-${member}-`));
const tarball = execFileSync('npm', ['pack', `@kanonak-protocol/${member}@${baseline}`, '--silent'], {
  cwd: work,
  encoding: 'utf8',
  shell: process.platform === 'win32',
})
  .trim()
  .split('\n')
  .pop();
mkdirSync(join(work, 'old'));
execFileSync('tar', ['xzf', tarball, '-C', 'old'], { cwd: work });

// Private members are not API, but TypeScript compares classes that declare
// them nominally, so two copies of one unchanged class would never match.
// Compare the public surface: copy both sides' declarations without them.
const PRIVATE_MEMBER = /^\s*(private\s.*;|#private;)\s*$/;
function publicDeclarations(from, to) {
  mkdirSync(to, { recursive: true });
  for (const entry of readdirSync(from)) {
    const src = join(from, entry);
    if (statSync(src).isDirectory()) {
      publicDeclarations(src, join(to, entry));
    } else if (entry.endsWith('.d.ts')) {
      const lines = readFileSync(src, 'utf8').split(/\r?\n/);
      writeFileSync(join(to, entry), lines.filter((line) => !PRIVATE_MEMBER.test(line)).join('\n'));
    }
  }
}
publicDeclarations(join(work, 'old', 'package', 'dist'), join(work, 'old-public'));
publicDeclarations(dist, join(work, 'new-public'));
const old = join(work, 'old-public', 'index.d.ts');
const fresh = join(work, 'new-public', 'index.d.ts');

const options = {
  strict: true,
  noEmit: true,
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
  target: ts.ScriptTarget.ES2022,
  types: [],
  // A member's declarations import its siblings by package name (codec ->
  // canonical); resolve those to the built candidates on both sides, so a
  // sibling type is compared, never silently `any`.
  baseUrl: root,
  paths: { '@kanonak-protocol/*': ['kanonak-*/typescript/dist/index.d.ts'] },
};

// The baseline's exports, values and types, through re-export aliases.
const oldProgram = ts.createProgram([old], options);
const checker = oldProgram.getTypeChecker();
const moduleSymbol = checker.getSymbolAtLocation(oldProgram.getSourceFile(old));
const exported = checker.getExportsOfModule(moduleSymbol);

const spec = (p) => p.replace(/\\/g, '/').replace(/\.d\.ts$/, '.js');
const lines = [
  `import type * as Old from '${spec(old)}';`,
  `import type * as New from '${spec(fresh)}';`,
  'type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;',
  `declare const candidate: typeof import('${spec(fresh)}');`,
];
let values = 0;
let types = 0;
for (const entry of exported) {
  const symbol = entry.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(entry) : entry;
  const name = entry.getName();
  if (symbol.flags & ts.SymbolFlags.Value) {
    lines.push(`export const value_${name}: typeof import('${spec(old)}').${name} = candidate.${name};`);
    values++;
  }
  if (symbol.flags & (ts.SymbolFlags.Interface | ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Class | ts.SymbolFlags.Enum)) {
    const declaration = symbol.declarations?.find((d) => 'typeParameters' in d);
    const arity = declaration?.typeParameters?.length ?? 0;
    const args = arity ? `<${Array(arity).fill('unknown').join(', ')}>` : '';
    lines.push(`export const type_${name}: Same<Old.${name}${args}, New.${name}${args}> = true;`);
    types++;
  }
}

const check = join(work, 'check.ts');
writeFileSync(check, lines.join('\n') + '\n');
const program = ts.createProgram([check], options);
const same = (a, b) => resolve(a).toLowerCase() === resolve(b).toLowerCase();
const diagnostics = ts.getPreEmitDiagnostics(program).filter((d) => d.file && same(d.file.fileName, check));
if (diagnostics.length) {
  for (const d of diagnostics) {
    const { line } = d.file.getLineAndCharacterOfPosition(d.start ?? 0);
    console.error(
      `::error::API BREAK @kanonak-protocol/${member} against ${baseline}: ` +
        `${ts.flattenDiagnosticMessageText(d.messageText, ' ')}\n    at: ${lines[line] ?? ''}`,
    );
  }
  process.exit(1);
}
console.log(`OK        typescript    @kanonak-protocol/${member}: ${values} values and ${types} types of ${baseline} stay compatible`);
