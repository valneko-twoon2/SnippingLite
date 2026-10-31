// Tests of mutate.mjs: which mutants it finds, how it scores them, and that the source file always comes back.
// Run from the skill folder: node --test "scripts/test/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(new URL('../mutate.mjs', import.meta.url));
const NODE = JSON.stringify(process.execPath);

// Lines 2 and 3 hold a `>=` only in a comment and in a string: neither may be mutated. Every mutant of lines 4 and 5
// changes what `size` returns for some number, so a test at the boundary catches them all. CRLF line ends on purpose:
// the file must come back byte for byte.
const SOURCE = [
  'export function size(n) {',
  '  // n >= 10 is big',
  "  const note = 'n >= 10';",
  '  if (n >= 10) return true;',
  '  return false;',
  '}',
  '',
].join('\r\n');

const weakTest = 'assert.equal(size(50), true); assert.equal(size(1), false);';
const strongTest = 'assert.equal(size(10), true); assert.equal(size(9), false); assert.equal(size(50), true); assert.equal(size(1), false);';

// A project in a temporary folder that is removed when the test ends, passed or failed.
function project(t, testBody) {
  const dir = mkdtempSync(join(tmpdir(), 'mutate-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, 'size.mjs'), SOURCE);
  writeFileSync(join(dir, 'size.test.mjs'), `import assert from 'node:assert/strict';\nimport { size } from './size.mjs';\n${testBody}\n`);
  return dir;
}

function mutate(dir, ...args) {
  return spawnSync(process.execPath, [SCRIPT, '--file', 'size.mjs', ...args], { cwd: dir, encoding: 'utf8' });
}

const runTests = `${NODE} --test size.test.mjs`;

// The --list output of one source file in a temporary folder.
function listOf(t, name, source) {
  const dir = mkdtempSync(join(tmpdir(), 'mutate-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, name), source);
  const r = spawnSync(process.execPath, [SCRIPT, '--list', '--file', name], { cwd: dir, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout;
}

test('decimals, and strings that hold only a number, move by one in their last digit; comments and other strings stay', (t) => {
  const out = listOf(t, 'rules.py', ['RATE = Decimal("0.15")  # 0.15 off', 'FEE = 100.00', 'NAME = "v1.0"', 'KEY = "__"'].join('\n'));
  assert.match(out, /rules\.py:1:17 {2}number {2}"0\.15" -> "0\.16"/);
  assert.match(out, /rules\.py:1:17 {2}number {2}"0\.15" -> "0\.14"/);
  assert.match(out, /rules\.py:2:7 {2}number {2}"100\.00" -> "100\.01"/);
  assert.match(out, /rules\.py:2:7 {2}number {2}"100\.00" -> "99\.99"/);
  assert.doesNotMatch(out, /rules\.py:1:27 /, 'the number in the comment');
  assert.doesNotMatch(out, /rules\.py:[34]:/, 'strings that are not only a number');
});

test('a whole Python condition is negated when it ends on its line, unless it is already a negation', (t) => {
  const out = listOf(t, 'p.py', ['if member:', '    pass', 'elif not member and n > 10:', '    pass', 'while n: n -= 1', 'if (a and', '        b):', '    pass'].join('\n'));
  assert.match(out, /p\.py:1:4 {2}condition {2}"member" -> "not \(member\)"/);
  assert.match(out, /p\.py:5:7 {2}condition {2}"n" -> "not \(n\)"/);
  assert.doesNotMatch(out, /p\.py:3:\d+ {2}condition/, 'already a negation');
  assert.doesNotMatch(out, /p\.py:6:\d+ {2}condition/, 'continues on the next line');
});

test('a C-like condition in parentheses is negated, unless it is already a negation', (t) => {
  const out = listOf(t, 'p.js', ['if (ok) {', '} else if (!ok) {', '}', 'while (i < n) i++;', "notify('if (x)');"].join('\n'));
  assert.match(out, /p\.js:1:5 {2}condition {2}"ok" -> "!\(ok\)"/);
  assert.match(out, /p\.js:4:8 {2}condition {2}"i < n" -> "!\(i < n\)"/);
  assert.doesNotMatch(out, /p\.js:[25]:\d+ {2}condition/);
});

test('a Rust or Go condition before the block brace is negated, but not `if let` or an `if` inside an expression', (t) => {
  const rs = listOf(t, 'p.rs', ['if ready {', 'if let Some(x) = y {', 'let a = if x { 1 } else { 2 };', 'while n > 0 {'].join('\n'));
  assert.match(rs, /p\.rs:1:4 {2}condition {2}"ready" -> "!\(ready\)"/);
  assert.match(rs, /p\.rs:4:7 {2}condition {2}"n > 0" -> "!\(n > 0\)"/);
  assert.doesNotMatch(rs, /p\.rs:[23]:\d+ {2}condition/);
  const go = listOf(t, 'p.go', 'if err := f(); err != nil {\n');
  assert.match(go, /p\.go:1:16 {2}condition {2}"err != nil" -> "!\(err != nil\)"/);
});

test('lists the mutants of code only, never of strings or comments', (t) => {
  const dir = project(t, weakTest);
  const r = mutate(dir, '--list');
  assert.equal(r.status, 0, r.stderr);
  const mutants = r.stdout.split('\n').filter((l) => l.startsWith('size.mjs:'));
  assert.ok(mutants.length > 0 && mutants.every((l) => !/^size\.mjs:[23]:/.test(l)), r.stdout);
  for (const change of ['" >= " -> " > "', '"10" -> "11"', '"10" -> "9"', '"true" -> "false"', '"false" -> "true"']) {
    assert.ok(r.stdout.includes(change), `${change} missing from:\n${r.stdout}`);
  }
});

test('a weak test leaves the boundary mutants alive, and the file is back byte for byte', (t) => {
  const dir = project(t, weakTest);
  const before = readFileSync(join(dir, 'size.mjs'));
  const r = mutate(dir, '--test', runTests);
  assert.equal(r.status, 1, r.stdout + r.stderr);
  // Not everything survives: `true -> false` breaks size(50), so a run that called every mutant missed is wrong.
  assert.match(r.stdout, /size\.mjs:4:23 "true" -> "false": caught/);
  assert.match(r.stdout, /Survivors/);
  // line:column of the changed token itself: `  if (n >= 10) return true;`
  assert.match(r.stdout, /size\.mjs:4:9 {2}" >= " -> " > "/);
  assert.match(r.stdout, /size\.mjs:4:12 {2}"10" -> "11"/);
  assert.deepEqual(readFileSync(join(dir, 'size.mjs')), before);
});

test('a test at the boundary kills every mutant', (t) => {
  const dir = project(t, strongTest);
  const r = mutate(dir, '--test', runTests);
  assert.match(r.stdout, /missed 0/, r.stdout + r.stderr);
  assert.equal(r.status, 0);
});

test('a mutant that hangs the tests is a timeout, and the file still comes back', (t) => {
  const dir = project(t, weakTest);
  // The "tests" hang whenever the file is mutated to `n > 10`.
  writeFileSync(join(dir, 'hang.mjs'), "import { readFileSync } from 'node:fs';\nif (readFileSync('size.mjs', 'utf8').includes('n > 10')) setInterval(() => {}, 1000);\n");
  const before = readFileSync(join(dir, 'size.mjs'));
  const r = mutate(dir, '--test', `${NODE} hang.mjs`, '--lines', '4', '--timeout', '2');
  assert.match(r.stdout, /timeout 1/, r.stdout + r.stderr);
  assert.match(r.stdout, /Timeouts/);
  assert.deepEqual(readFileSync(join(dir, 'size.mjs')), before);
});

test('tests that fail before any mutation stop the run with their output', (t) => {
  const dir = project(t, 'assert.equal(size(10), false);');
  const r = mutate(dir, '--test', runTests);
  assert.equal(r.status, 2, r.stdout + r.stderr);
  assert.match(r.stderr, /fail before any mutation/);
});

test('a sampled run spreads over the range and says how many there were', (t) => {
  const dir = project(t, weakTest);
  const r = mutate(dir, '--list', '--max', '2');
  assert.match(r.stdout, /2 mutant\(s\) \(sampled from \d+\)/);
});

test('bad options are refused with what to give instead', (t) => {
  const dir = project(t, weakTest);
  assert.match(mutate(dir, '--lines', 'ten').stderr, /--lines takes/);
  assert.match(mutate(dir).stderr, /--test "<command>" is required/);
  assert.match(mutate(dir, '--list', '--max', '0').stderr, /--max takes/);
});

const hasGit = spawnSync('git', ['--version']).status === 0;

test('a file with uncommitted changes is refused unless allowed', { skip: !hasGit && 'git is not installed' }, (t) => {
  const dir = project(t, strongTest);
  const git = (...a) => spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.invalid', ...a], { cwd: dir });
  git('init', '-q');
  git('add', '-A');
  git('commit', '-q', '-m', 'start');
  writeFileSync(join(dir, 'size.mjs'), SOURCE + '\r\n');
  const refused = mutate(dir, '--test', runTests, '--lines', '4');
  assert.equal(refused.status, 2);
  assert.match(refused.stderr, /uncommitted changes/);
  const allowed = mutate(dir, '--test', runTests, '--lines', '4', '--allow-dirty');
  assert.equal(allowed.status, 0, allowed.stdout + allowed.stderr);
});

test('a run left unfinished blocks the next one until --restore puts the original back', (t) => {
  const dir = project(t, weakTest);
  // What a killed run leaves behind: its record and the saved original, with the file still mutated.
  const file = join(dir, 'size.mjs');
  const id = createHash('sha1').update(file.toLowerCase()).digest('hex').slice(0, 16);
  const guardDir = join(tmpdir(), 'mutate-guard');
  mkdirSync(guardDir, { recursive: true });
  t.after(() => {
    for (const ext of ['orig', 'json']) rmSync(join(guardDir, `${id}.${ext}`), { force: true });
  });
  writeFileSync(join(guardDir, `${id}.orig`), SOURCE);
  writeFileSync(join(guardDir, `${id}.json`), JSON.stringify({ file }));
  writeFileSync(file, SOURCE.replace('n >= 10)', 'n > 10)'));
  const blocked = mutate(dir, '--test', runTests);
  assert.equal(blocked.status, 2);
  assert.match(blocked.stderr, /--restore/);
  const restored = mutate(dir, '--restore');
  assert.equal(restored.status, 0, restored.stderr);
  assert.equal(readFileSync(file, 'utf8'), SOURCE);
  assert.equal(existsSync(join(guardDir, `${id}.json`)), false);
});

test('a Python source never runs from older bytecode: its cache goes after every write, and imports write none', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'mutate-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const cache = join(dir, '__pycache__');
  mkdirSync(cache);
  const source = 'def size(n):\n    return n >= 10\n';
  writeFileSync(join(dir, 'size.py'), source);
  writeFileSync(join(cache, 'size.cpython-310.pyc'), source); // compiled from the original, before the run
  writeFileSync(join(cache, 'other.cpython-310.pyc'), 'another module');
  // Stands in for Python: a cache whose content is not the source's is stale bytecode, which would run the wrong
  // code (exit 3). Then it "compiles" the source into the cache, as an import does.
  writeFileSync(join(dir, 'fake.mjs'), [
    "import { existsSync, readFileSync, writeFileSync } from 'node:fs';",
    "const pyc = '__pycache__/size.cpython-310.pyc';",
    "if (process.env.PYTHONDONTWRITEBYTECODE !== '1') process.exit(4);",
    "if (existsSync(pyc) && readFileSync(pyc, 'utf8') !== readFileSync('size.py', 'utf8')) process.exit(3);",
    "writeFileSync(pyc, readFileSync('size.py', 'utf8'));",
  ].join('\n'));
  const r = spawnSync(process.execPath, [SCRIPT, '--file', 'size.py', '--test', `${NODE} fake.mjs`], { cwd: dir, encoding: 'utf8' });
  assert.match(r.stdout, /Mutants: 3: caught 0, missed 3/, r.stdout + r.stderr);
  assert.equal(existsSync(join(cache, 'size.cpython-310.pyc')), false, 'no bytecode of a mutant is left');
  assert.equal(readFileSync(join(cache, 'other.cpython-310.pyc'), 'utf8'), 'another module');
});

test('--help prints the usage', () => {
  const r = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(r.status, 0);
  assert.match(r.stdout, /Usage:\n {2}node mutate\.mjs --file/);
});
