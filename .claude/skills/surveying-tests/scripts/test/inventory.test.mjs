// Tests of inventory.mjs on a made-up repository: what it finds, what it must not mistake for a test kind, and that
// ignored files stay out. Run from the skill folder: node --test "scripts/test/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(new URL('../inventory.mjs', import.meta.url));

const FILES = {
  'package.json': JSON.stringify({
    name: 'x',
    scripts: { test: 'vitest run', coverage: 'vitest run --coverage', build: 'tsc' },
    devDependencies: { 'fast-check': '^3', '@playwright/test': '^1' },
  }),
  'playwright.config.ts': "export default { projects: [{ name: 'chromium' }, { name: 'webkit' }] };\n",
  // A configuration key named coverage: configuration, not a dependency.
  'vitest.config.ts': "export default { test: { coverage: { provider: 'v8' } } };\n",
  'src/money.ts': 'export const a = 1;\n'.repeat(30),
  'src/money.test.ts': "import fc from 'fast-check';\nfc.assert(fc.property(fc.integer(), () => true));\n",
  'src/big.ts': 'export const b = 2;\n'.repeat(100),
  'src/index.ts': "export * from './money';\n".repeat(200),
  // Ordinary code whose name has "crash" in it: not a crash-consistency test.
  'src/crash-report.ts': 'export const report = 0;\n'.repeat(10),
  'e2e/flow.spec.ts': "import { test } from '@playwright/test';\n",
  'crates/x/Cargo.toml': '[package]\nname = "x"\n[dev-dependencies]\nproptest = "1"\n',
  'crates/x/src/lib.rs': 'pub fn f() {}\n#[cfg(test)]\nmod tests {}\n',
  'crates/x/fuzz/fuzz_targets/parse.rs': 'fuzz_target!(|data: &[u8]| {});\n',
  'tests/test_rules.py': 'from hypothesis import given\n@given()\ndef test_x(): pass\n',
  '.github/workflows/ci.yml': 'jobs:\n  t:\n    steps:\n      - run: npm test\n      - run: cargo test --workspace\n',
};

// The made-up repository in a temporary folder that is removed when the test ends, passed or failed.
function repository(t, extra = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'inventory-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const [path, text] of Object.entries({ ...FILES, ...extra })) {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  }
  return dir;
}

const inventory = (dir) => spawnSync(process.execPath, [SCRIPT, dir], { encoding: 'utf8' });
const row = (out, kind) => out.split('\n').find((l) => l.startsWith(`| ${kind} |`)) ?? '';

test('finds the languages, the test runner commands and the CI steps', (t) => {
  const r = inventory(repository(t));
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| TypeScript \| \d+ \| 2 \|/);
  assert.match(r.stdout, /\| Rust \| \d+ \| \d+ \| 1 \|/);
  assert.match(r.stdout, /\| Python \| 0 \| 1 \|/);
  assert.match(r.stdout, /`test`: `vitest run`/);
  assert.doesNotMatch(r.stdout, /`build`/, 'only test-related scripts are listed');
  assert.match(r.stdout, /`- run: cargo test --workspace`/);
});

test('names the evidence for each kind it finds', (t) => {
  const out = inventory(repository(t)).stdout;
  // Every matching dependency is named, from every manifest, in no particular order.
  assert.match(row(out, 'property-based'), /dependencies [^;]*`fast-check`/);
  assert.match(row(out, 'property-based'), /dependencies [^;]*`proptest`/);
  assert.match(row(out, 'property-based'), /src\/money\.test\.ts/);
  assert.match(row(out, 'fuzzing'), /crates\/x\/fuzz\/fuzz_targets\/parse\.rs/);
  assert.match(row(out, 'end-to-end (browser or app)'), /@playwright\/test/);
  assert.match(row(out, 'cross-browser / device'), /webkit/);
  assert.match(row(out, 'coverage measurement'), /configuration or CI: `--coverage`/);
});

test('does not mistake a script name or an ordinary file name for a kind of test', (t) => {
  const out = inventory(repository(t)).stdout;
  // "coverage" is a script name in package.json, not a dependency.
  assert.doesNotMatch(row(out, 'coverage measurement'), /dependency/);
  assert.match(row(out, 'crash consistency / fault injection'), /none found/);
  assert.match(row(out, 'mutation testing'), /none found/);
});

test('lists the largest source files with no test of their name, leaving out index files', (t) => {
  const out = inventory(repository(t)).stdout;
  const section = out.slice(out.indexOf('## Largest source files'));
  assert.match(section, /- src\/big\.ts \(101 lines\)/);
  assert.doesNotMatch(section, /src\/money\.ts|src\/index\.ts|crates\/x\/src\/lib\.rs/);
});

test('a folder that does not exist is refused', () => {
  const r = inventory(join(tmpdir(), 'inventory-test-does-not-exist'));
  assert.equal(r.status, 2);
  assert.match(r.stderr, /is not a folder/);
});

const hasGit = spawnSync('git', ['--version']).status === 0;

test('in a git repository, ignored files are left out, and the folders it skips are counted', { skip: !hasGit && 'git is not installed' }, (t) => {
  const dir = repository(t, { '.gitignore': 'ignored/\n', 'ignored/skip.test.ts': 'x\n', '.claude/a.md': 'x\n', '.claude/b/c.md': 'x\n' });
  spawnSync('git', ['init', '-q'], { cwd: dir });
  const out = inventory(dir).stdout;
  assert.match(out, /Files listed by git ls-files: \d+ \(left out: 2 under \.claude\)\./);
  assert.doesNotMatch(out, /ignored\/skip\.test\.ts/);
  assert.match(out, /\| TypeScript \| \d+ \| 2 \|/, 'the ignored test file is not counted');
});

test('a walk names the folders it did not read', (t) => {
  const out = inventory(repository(t, { 'node_modules/x/index.js': 'x\n' })).stdout;
  assert.match(out, /Files listed by a walk of the folder \(not a git repository\): \d+ \(left out: the folders node_modules\)\./);
});

test('test commands written in the README are listed when nothing else says how to run the tests', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'inventory-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const readme = ['# Pricing', '', 'This library is well tested.', '', '```bash', '$ python -m unittest discover -s tests -t .', 'echo done', '```', '', 'Or run `pytest -q` for short output, after `pip install .`.', ''].join('\n');
  writeFileSync(join(dir, 'README.md'), readme);
  const out = inventory(dir).stdout;
  assert.match(out, /No manifest or test configuration found\./);
  assert.match(out, /- Test commands written in README\.md:\n {2}- `python -m unittest discover -s tests -t \.`\n {2}- `pytest -q`\n/);
  assert.doesNotMatch(out, /well tested|echo done|pip install/);
});

test('--help prints the usage', () => {
  const r = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(r.status, 0);
  assert.match(r.stdout, /Usage: node inventory\.mjs/);
});

test('an installed copy of this skill under .claude is not counted as the project tests', (t) => {
  const out = inventory(repository(t, { '.claude/skills/surveying-tests/scripts/test/mutate.test.mjs': 'x\n' })).stdout;
  assert.doesNotMatch(out, /\.claude\//);
  assert.doesNotMatch(out, /\| JavaScript \|/);
});
