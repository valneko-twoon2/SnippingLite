// Tests of coverage-gaps.mjs: each report format, the ranking, and the files a report leaves out.
// Run from the skill folder: node --test "scripts/test/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(new URL('../coverage-gaps.mjs', import.meta.url));

function gaps(dir, report, ...args) {
  writeFileSync(join(dir, 'report'), report);
  return spawnSync(process.execPath, [SCRIPT, 'report', ...args], { cwd: dir, encoding: 'utf8' });
}

// A temporary folder that is removed when the test ends, passed or failed.
function folder(t) {
  const dir = mkdtempSync(join(tmpdir(), 'coverage-gaps-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

// a.ts: lines 1-4 run, 5-6 never; one branch of two taken on line 2. b.ts: all lines run, both branches taken.
const LCOV = [
  'TN:',
  'SF:src/a.ts',
  'DA:1,1', 'DA:2,3', 'DA:3,1', 'DA:4,1', 'DA:5,0', 'DA:6,0',
  'BRDA:2,0,0,3', 'BRDA:2,0,1,-',
  'end_of_record',
  'SF:src/b.ts',
  'DA:1,1', 'DA:2,1',
  'BRDA:1,0,0,1', 'BRDA:1,0,1,2',
  'end_of_record',
  '',
].join('\n');

// One lcov record per file with gaps, `count` files: f0.js, f1.js, ...
const gapFiles = (count) => Array.from({ length: count }, (_, i) => `SF:f${i}.js\nDA:1,0\nend_of_record`).join('\n');

const listed = (stdout) => [...stdout.matchAll(/^\| (f\d+\.js) \|/gm)].map((m) => m[1]);

// An lcov record: the hit count of lines 1, 2, ... and the taken count of each branch on line 1.
const record = (name, hits, taken) =>
  [`SF:${name}`, ...hits.map((h, i) => `DA:${i + 1},${h}`), ...taken.map((n, i) => `BRDA:1,0,${i},${n}`), 'end_of_record'].join('\n');

test('lcov: totals over every file, and only the files with a gap are listed', (t) => {
  const r = gaps(folder(t), LCOV);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\(lcov\): 2 file/);
  assert.match(r.stdout, /Lines: 6 of 8 \(75\.0%\)/);
  assert.match(r.stdout, /Branches: 3 of 4 \(75\.0%\)/);
  assert.match(r.stdout, /\| src\/a\.ts \| 4\/6 \(66\.7%\) \| 1\/2 \(50\.0%\) \| 5-6 \| 2 \|/);
  assert.doesNotMatch(r.stdout, /\| src\/b\.ts/, 'a file with no gap is not listed');
});

test('lcov: the function and summary lines of a real record do not end it', (t) => {
  // The order c8, nyc and istanbul write: functions, lines, line totals, branches, branch totals.
  const record = ['TN:', 'SF:src/r.ts', 'FN:1,(empty-report)', 'FN:3,total', 'FNF:2', 'FNH:1', 'FNDA:0,(empty-report)',
    'FNDA:1,total', 'DA:1,1', 'DA:2,0', 'DA:3,1', 'LF:3', 'LH:2', 'BRDA:3,0,0,1', 'BRDA:3,0,1,0', 'BRF:2', 'BRH:1',
    'end_of_record', ''].join('\n');
  const r = gaps(folder(t), record);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| src\/r\.ts \| 2\/3 \(66\.7%\) \| 1\/2 \(50\.0%\) \| 2 \| 3 \|/);
});

test('lcov: a file listed twice keeps the larger count of each line', (t) => {
  const twice = 'SF:x.js\nDA:1,0\nDA:2,0\nend_of_record\nSF:x.js\nDA:1,4\nDA:2,0\nend_of_record\n';
  const r = gaps(folder(t), twice);
  assert.match(r.stdout, /Lines: 1 of 2/);
  assert.match(r.stdout, /Branches: none in this report/);
});

test('lcov: a file listed twice keeps every branch that either record took', (t) => {
  // Reports of several test processes are often joined into one file, so the same file comes once per process.
  const twice = 'SF:x.js\nDA:1,1\nBRDA:1,0,0,1\nBRDA:1,0,1,0\nend_of_record\nSF:x.js\nDA:1,1\nBRDA:1,0,0,0\nBRDA:1,0,1,1\nend_of_record\n';
  const r = gaps(folder(t), twice);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /Branches: 2 of 2 \(100\.0%\)/);
});

test('lcov: a report cut off inside its last record is refused instead of ranked', (t) => {
  const r = gaps(folder(t), `${LCOV}SF:src/c.ts\nDA:1,0\n`);
  assert.equal(r.status, 2);
  assert.match(r.stderr, /cut off/);
  assert.equal(r.stdout, '');
});

test('Cobertura: a line repeated under its method is counted once, and branches come from condition-coverage', (t) => {
  const dir = folder(t);
  const xml = `<?xml version="1.0"?>
<coverage line-rate="0.5"><sources><source>${dir}</source></sources><packages><package name="p"><classes>
<class name="m" filename="pkg/m.py"><methods><method name="f"><lines>
  <line number="1" hits="1"/><line number="2" hits="0"/></lines></method></methods>
<lines><line number="1" hits="1" branch="true" condition-coverage="50% (1/2)"/><line number="2" hits="0"/><line number="3" hits="2"/></lines>
</class></classes></package></packages></coverage>`;
  const r = gaps(dir, xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\(Cobertura XML\)/);
  assert.match(r.stdout, /Lines: 2 of 3/);
  assert.match(r.stdout, /Branches: 1 of 2/);
  assert.match(r.stdout, /m\.py \| 2\/3 .*\| 2 \| 1 \|/);
});

for (const [inMethod, inClass] of [['50% (1/2)', '100% (2/2)'], ['100% (2/2)', '50% (1/2)']]) {
  test(`Cobertura: a branch line given as ${inMethod} and as ${inClass} keeps the one with more branches taken`, (t) => {
    const line = (cc) => `<line number="1" hits="1" branch="true" condition-coverage="${cc}"/>`;
    const xml = `<coverage><packages><package name="p"><classes><class name="m" filename="m.py">
<methods><method name="f"><lines>${line(inMethod)}</lines></method></methods><lines>${line(inClass)}</lines>
</class></classes></package></packages></coverage>`;
    const r = gaps(folder(t), xml);
    assert.equal(r.status, 0, r.stderr);
    assert.match(r.stdout, /Branches: 2 of 2 \(100\.0%\)/);
  });
}

test('Cobertura: a relative file name is read under the first <source> folder that has it', (t) => {
  const dir = folder(t);
  mkdirSync(join(dir, 'lib', 'pkg'), { recursive: true });
  for (const f of ['m.py', 'n.py']) writeFileSync(join(dir, 'lib', 'pkg', f), '');
  const xml = `<?xml version="1.0"?>
<coverage><sources><source>${join(dir, 'nothing-here')}</source><source>${join(dir, 'lib')}</source></sources>
<packages><package name="pkg"><classes><class name="m.py" filename="pkg/m.py"><methods/><lines><line number="1" hits="0"/></lines>
</class></classes></package></packages></coverage>`;
  const r = gaps(dir, xml, '--expect', 'lib');
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| lib\/pkg\/m\.py \| 0\/1 \(0\.0%\) \| - \| 1 \| - \|/);
  assert.match(r.stdout, /Not in the report at all .*: 1\n {2}lib\/pkg\/n\.py\n/);
});

test('Cobertura: a line with no hits attribute counts as never run', (t) => {
  const xml = '<coverage><packages><package name="p"><classes><class name="m" filename="m.py"><lines><line number="1" hits="1"/><line number="2"/></lines></class></classes></package></packages></coverage>';
  const r = gaps(folder(t), xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| m\.py \| 1\/2 \(50\.0%\) \| - \| 2 \| - \|/);
});

test('Cobertura: a branch line with no condition-coverage adds no branch', (t) => {
  const xml = '<coverage><packages><package name="p"><classes><class name="m" filename="m.py"><lines><line number="1" hits="1" branch="true"/></lines></class></classes></package></packages></coverage>';
  const r = gaps(folder(t), xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /Branches: none in this report/);
});

test('JaCoCo: a line with no covered instruction is a missed line; mb and cb are its branches', (t) => {
  // As JaCoCo writes it: <class> elements with counters next to each <sourcefile>.
  const xml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><!DOCTYPE report PUBLIC "-//JACOCO//DTD Report 1.1//EN" "report.dtd">
<report name="r"><sessioninfo id="s" start="1" dump="2"/><package name="com/x">
<class name="com/x/A" sourcefilename="A.java"><method name="f" desc="()V" line="3"><counter type="INSTRUCTION" missed="2" covered="5"/></method><counter type="CLASS" missed="0" covered="1"/></class>
<sourcefile name="A.java"><line nr="3" mi="0" ci="4" mb="1" cb="1"/><line nr="4" mi="2" ci="0" mb="0" cb="0"/><line nr="5" mi="0" ci="1" mb="1" cb="0"/><counter type="LINE" missed="1" covered="2"/></sourcefile>
</package></report>`;
  const r = gaps(folder(t), xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\(JaCoCo XML\)/);
  assert.match(r.stdout, /\| com\/x\/A\.java \| 2\/3 \(66\.7%\) \| 1\/3 \(33\.3%\) \| 4 \| 3, 5 \|/);
});

test('JaCoCo: a line with no counters counts as never run and has no branch', (t) => {
  const xml = '<report name="r"><package name="p"><sourcefile name="B.java"><line nr="1" ci="1"/><line nr="2"/></sourcefile></package></report>';
  const r = gaps(folder(t), xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| p\/B\.java \| 1\/2 \(50\.0%\) \| - \| 2 \| - \|/);
  assert.match(r.stdout, /Branches: none in this report/);
});

test('Go: a block marks every line it spans, and the report says it has no branches', (t) => {
  const profile = 'mode: set\nexample.com/m/a.go:3.10,5.2 2 1\nexample.com/m/a.go:6.2,8.3 1 0\n';
  const r = gaps(folder(t), profile);
  assert.match(r.stdout, /\(Go cover profile\)/);
  assert.match(r.stdout, /Lines: 3 of 6/);
  assert.match(r.stdout, /Branches: none in this report/);
  assert.match(r.stdout, /a\.go \| 3\/6 \(50\.0%\) \| - \| 6-8 \| - \|/);
});

for (const [format, empty] of [
  ['lcov', 'TN:\n'],
  ['Cobertura XML', '<?xml version="1.0"?>\n<coverage line-rate="0"><sources/><packages/></coverage>\n'],
  ['JaCoCo XML', '<report name="r"><sessioninfo id="s" start="1" dump="2"/></report>\n'],
  ['Go cover profile', 'mode: set\n'],
]) {
  test(`an empty ${format} report is refused as naming no source files`, (t) => {
    const r = gaps(folder(t), empty);
    assert.equal(r.status, 2);
    assert.match(r.stderr, new RegExp(`\\(${format}\\) names no source files`));
  });
}

test('an empty report with --expect lists every source file as not loaded, the baseline of a project with no tests', (t) => {
  const dir = folder(t);
  mkdirSync(join(dir, 'src'));
  for (const f of ['a.ts', 'b.ts']) writeFileSync(join(dir, 'src', f), '');
  const r = gaps(dir, 'TN:\n', '--expect', 'src', '--ext', '.ts');
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\(lcov\): 0 file\(s\)\nNo test loaded any source file\.\n/);
  assert.match(r.stdout, /Not in the report at all .*: 2\n {2}src\/a\.ts\n {2}src\/b\.ts\n/);
  const noExt = gaps(dir, 'TN:\n', '--expect', 'src');
  assert.equal(noExt.status, 2);
  assert.match(noExt.stderr, /--ext/);
});

test("Python trace folder: counts and never-run marks are read, and modules outside the root are left out", (t) => {
  const dir = folder(t);
  mkdirSync(join(dir, 'pricing'));
  for (const f of ['__init__.py', 'rules.py', 'extra.py']) writeFileSync(join(dir, 'pricing', f), '');
  mkdirSync(join(dir, '.trace'));
  // As `python -m trace --count --missing --coverdir=.trace` writes them: "%5d: " for a line that ran, ">>>>>> " for
  // one that never ran, seven spaces for a line with no code (here one that would read as a count without them).
  const cover = ['    1: def price(n):', '    3:     if n > 10:', '>>>>>>         return 1', '    3:     return 2', '          12: not a count'].join('\n');
  writeFileSync(join(dir, '.trace', 'pricing.rules.cover'), cover);
  writeFileSync(join(dir, '.trace', 'pricing.__init__.cover'), '    1: from .rules import price\n');
  writeFileSync(join(dir, '.trace', 'unittest.case.cover'), '>>>>>> class TestCase:\n');
  const r = spawnSync(process.execPath, [SCRIPT, '.trace', '--expect', 'pricing'], { cwd: dir, encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\(Python trace folder\): 2 file\(s\)/);
  assert.match(r.stdout, /\| pricing\/rules\.py \| 3\/4 \(75\.0%\) \| - \| 3 \| - \|/);
  assert.match(r.stdout, /Branches: none in this report/);
  assert.match(r.stdout, /Not in the report at all .*: 1\n {2}pricing\/extra\.py\n/);
});

test('files are ranked by missed branches, then by lines never run, whatever order the report lists them in', (t) => {
  const report = [
    record('f3.js', [1, 0, 0], [1, '-']), // 1 branch of 2 missed, 2 lines never run
    record('f4.js', [0, 0, 0, 0, 0, 0, 0, 0, 0], []), // no branch, 9 lines never run
    record('f1.js', [1, 0, 0, 0, 0, 0], [1, 1, 1, 1, 1, 0]), // 1 branch of 6 missed, 5 lines never run
    record('f5.js', [1], [1, 1]), // nothing missed
    record('f2.js', [1, 1], [1, 0, 0, 0]), // 3 branches of 4 missed, every line run
  ].join('\n');
  const r = gaps(folder(t), report);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(listed(r.stdout), ['f2.js', 'f1.js', 'f3.js', 'f4.js']);
  assert.match(r.stdout, /top 4 of 4/);
  assert.match(r.stdout, /\| f2\.js \| 2\/2 \(100\.0%\) \| 1\/4 \(25\.0%\) \| - \| 1 \|/);
});

test('the rank counts missed branches, not branches found or taken, also when the report is already in order', (t) => {
  const report = [
    record('f2.js', [1, 1], [1, 0, 0, 0]), // 3 branches of 4 missed
    record('f1.js', [1, 1], [1, 1, 1, 1, 1, 0]), // 1 branch of 6 missed: more branches found and taken
  ].join('\n');
  const r = gaps(folder(t), report);
  assert.deepEqual(listed(r.stdout), ['f2.js', 'f1.js']);
});

test('line ranges join only consecutive lines, in line order when the report lists methods out of order', (t) => {
  const line = (n, hits, cc) => `<line number="${n}" hits="${hits}"${cc ? ` branch="true" condition-coverage="${cc}"` : ''}/>`;
  const late = [line(12, 0), line(13, 0), line(14, 0, '50% (1/2)')];
  const early = [line(1, 1, '100% (2/2)'), line(3, 0), line(4, 0), line(5, 0), line(9, 0, '0% (0/2)')];
  const xml = `<coverage><packages><package name="p"><classes><class name="m" filename="m.py"><methods>
<method name="late"><lines>${late.join('')}</lines></method><method name="early"><lines>${early.join('')}</lines></method>
</methods><lines>${[...early, ...late].join('')}</lines></class></classes></package></packages></coverage>`;
  const r = gaps(folder(t), xml);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /\| m\.py \| 1\/8 \(12\.5%\) \| 3\/6 \(50\.0%\) \| 3-5, 9, 12-14 \| 9, 14 \|/);
});

test('without --top the 20 files with the most gaps are listed', (t) => {
  const r = gaps(folder(t), gapFiles(21));
  assert.match(r.stdout, /top 20 of 21/);
  assert.equal(listed(r.stdout).length, 20);
});

test('--top 1 lists the one file with the most gaps', (t) => {
  const r = gaps(folder(t), gapFiles(3), '--top', '1');
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /top 1 of 3/);
  assert.equal(listed(r.stdout).length, 1);
});

test('--top refuses 0 and a fraction', (t) => {
  const dir = folder(t);
  const zero = gaps(dir, gapFiles(3), '--top', '0');
  assert.equal(zero.status, 2);
  assert.match(zero.stderr, /--top takes a whole number of at least 1/);
  const fraction = gaps(dir, gapFiles(3), '--top', '1.5');
  assert.equal(fraction.status, 2);
  assert.match(fraction.stderr, /--top takes a whole number of at least 1/);
});

test('an option given last without its value is refused by name', (t) => {
  const dir = folder(t);
  const top = gaps(dir, gapFiles(1), '--top');
  assert.equal(top.status, 2);
  assert.match(top.stderr, /--top needs a value/);
  const expect = gaps(dir, gapFiles(1), '--expect');
  assert.equal(expect.status, 2);
  assert.match(expect.stderr, /--expect needs a value/);
});

test('--expect lists the source files no test loaded, and leaves out tests, dependencies and hidden folders', (t) => {
  const dir = folder(t);
  for (const d of ['deep', 'node_modules', '.cache']) mkdirSync(join(dir, 'src', d), { recursive: true });
  const files = ['a.ts', 'b.ts', 'c.ts', 'a.test.ts', 'types.d.ts', join('deep', 'd.ts'), join('node_modules', 'dep.ts'), join('.cache', 'gen.ts')];
  for (const f of files) writeFileSync(join(dir, 'src', f), '');
  const r = gaps(dir, LCOV, '--expect', 'src');
  assert.match(r.stdout, /Not in the report at all .*: 2\n/);
  assert.match(r.stdout, /\n {2}src\/c\.ts/);
  assert.match(r.stdout, /\n {2}src\/deep\/d\.ts/);
  assert.doesNotMatch(r.stdout, /a\.test\.ts|types\.d\.ts|dep\.ts|gen\.ts/);
});

test('--expect tells test files by their path inside the folder, so a project kept under a folder named tests is read', (t) => {
  const dir = join(folder(t), 'tests', 'proj');
  mkdirSync(join(dir, 'src'), { recursive: true });
  for (const f of ['a.ts', 'b.ts', 'c.ts']) writeFileSync(join(dir, 'src', f), '');
  const r = gaps(dir, LCOV, '--expect', 'src');
  assert.match(r.stdout, /Not in the report at all .*: 1\n {2}src\/c\.ts\n/);
});

test('--expect says so when every source file is in the report', (t) => {
  const dir = folder(t);
  mkdirSync(join(dir, 'src'));
  for (const f of ['a.ts', 'b.ts']) writeFileSync(join(dir, 'src', f), '');
  const r = gaps(dir, LCOV, '--expect', 'src');
  assert.match(r.stdout, /\nEvery \.ts source file under src is in the report\.\n/);
  assert.doesNotMatch(r.stdout, /Not in the report/);
});

test('report paths and files on disk are compared without case on Windows, and with case elsewhere', (t) => {
  const dir = folder(t);
  mkdirSync(join(dir, 'src'));
  writeFileSync(join(dir, 'src', 'a.ts'), '');
  const r = gaps(dir, 'SF:SRC/A.TS\nDA:1,0\nend_of_record\n', '--expect', 'src');
  const expected = process.platform === 'win32' ? /Every \.ts source file under src is in the report/ : /Not in the report at all .*: 1\n {2}src\/a\.ts/;
  assert.match(r.stdout, expected);
});

test('a file that is no coverage report is refused with the formats it can read', (t) => {
  const r = gaps(folder(t), '{"total": 1}');
  assert.equal(r.status, 2);
  assert.match(r.stderr, /not lcov, Cobertura XML, JaCoCo XML, a Go cover profile or a Python trace folder/);
  assert.equal(spawnSync(process.execPath, [SCRIPT], { encoding: 'utf8' }).status, 2);
});

test('--help prints the whole header comment as plain text, and an unknown option points at it', () => {
  const help = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /^Ranks what a coverage report says no test reached/);
  assert.match(help.stdout, /\nUsage: node coverage-gaps\.mjs <report or trace folder>/);
  assert.match(help.stdout, /Exit code: 0 read, 2 the report could not be read\.\n$/);
  assert.doesNotMatch(help.stdout, /^\/\/|^import /m);
  assert.match(spawnSync(process.execPath, [SCRIPT, 'r', '--nope'], { encoding: 'utf8' }).stderr, /see --help/);
});
