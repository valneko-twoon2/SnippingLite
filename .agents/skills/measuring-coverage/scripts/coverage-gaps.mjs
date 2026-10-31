#!/usr/bin/env node
// Ranks what a coverage report says no test reached, file by file, so the gaps are read in order instead of as one
// percentage. Reads lcov (`lcov.info`: c8, nyc, vitest, jest, `node --test --test-reporter=lcov`, cargo llvm-cov,
// coverage.py `coverage lcov`), Cobertura XML (coverage.py `coverage xml`, coverlet, ReportGenerator), JaCoCo XML, Go
// cover profiles (`go test -coverprofile`) and the folder Python's trace writes (`--coverdir`, lines only). With
// --expect <folder>, it also lists the source files the report does not name at all: no test loaded them, and many
// tools leave such files out instead of counting them as 0% (a report of a run with no tests yet lists them all).
//
// Usage: node coverage-gaps.mjs <report or trace folder> [--top 20] [--expect <folder>] [--ext .ts,.js] [--root <folder>]
//   --root   the folder report paths are relative to (default: the current folder)
//   --ext    the source extensions --expect looks for (default: the extensions of the files in the report)
// Needs Node 18 or later. Exit code: 0 read, 2 the report could not be read.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve, sep } from 'node:path';

function fail(message) {
  console.error(`coverage-gaps: ${message}`);
  process.exit(2);
}

function parseArgs(argv) {
  const args = { top: 20, root: process.cwd() };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) fail(`${a} needs a value`);
      return argv[++i];
    };
    if (a === '--top') {
      args.top = Number(value());
      if (!Number.isInteger(args.top) || args.top < 1) fail('--top takes a whole number of at least 1');
    } else if (a === '--expect') args.expect = value();
    else if (a === '--ext') args.ext = value().split(',').map((e) => (e.startsWith('.') ? e : `.${e}`).toLowerCase());
    else if (a === '--root') args.root = resolve(value());
    else if (a === '--help' || a === '-h') {
      const header = readFileSync(new URL(import.meta.url), 'utf8').split(/\r?\n/).slice(1);
      console.log(header.slice(0, header.findIndex((l) => !l.startsWith('//'))).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
      process.exit(0);
    } else if (a.startsWith('--')) fail(`unknown option ${a}; see --help`);
    else if (!args.report) args.report = a;
    else fail(`only one report at a time (got ${args.report} and ${a})`);
  }
  if (!args.report) fail('give the coverage report: node coverage-gaps.mjs <lcov.info | cobertura.xml | jacoco.xml | cover.out | trace folder>');
  return args;
}

/** One file's data: hits per line, and per line the branches found and taken. */
function entry(files, path) {
  if (!files.has(path)) files.set(path, { lines: new Map(), branches: new Map() });
  return files.get(path);
}

// A line seen twice (Cobertura repeats lines under each method; lcov may list a file twice) keeps the larger count.
function addLine(file, line, hits) {
  file.lines.set(line, Math.max(file.lines.get(line) ?? 0, hits));
}

function addBranches(file, line, found, taken) {
  const before = file.branches.get(line);
  if (!before || found > before.found || (found === before.found && taken > before.taken)) file.branches.set(line, { found, taken });
}

function parseLcov(text) {
  const files = new Map();
  let file = null;
  const branchHits = new Map(); // per file: "line,block,branch" -> taken, summed into lines below
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith('SF:')) {
      file = entry(files, line.slice(3));
      if (!branchHits.has(file)) branchHits.set(file, new Map()); // a file listed again adds to its earlier branches
    } else if (line.startsWith('DA:') && file) {
      const [n, hits] = line.slice(3).split(',');
      addLine(file, Number(n), Number(hits));
    } else if (line.startsWith('BRDA:') && file) {
      const [n, block, branch, taken] = line.slice(5).split(',');
      const key = `${n},${block},${branch}`;
      const count = taken === '-' ? 0 : Number(taken);
      const map = branchHits.get(file);
      map.set(key, Math.max(map.get(key) ?? 0, count));
    } else if (line === 'end_of_record') file = null;
  }
  for (const [f, map] of branchHits) {
    const perLine = new Map();
    for (const [key, count] of map) {
      const n = Number(key.split(',')[0]);
      const b = perLine.get(n) ?? { found: 0, taken: 0 };
      b.found += 1;
      if (count > 0) b.taken += 1;
      perLine.set(n, b);
    }
    for (const [n, b] of perLine) addBranches(f, n, b.found, b.taken);
  }
  return files;
}

const attr = (tag, name) => new RegExp(`\\b${name}="([^"]*)"`).exec(tag)?.[1];

function parseCobertura(text) {
  const files = new Map();
  const sources = [...text.matchAll(/<source>([^<]*)<\/source>/g)].map(([, s]) => s.trim());
  for (const [, classAttrs, body] of text.matchAll(/<class\b([^>]*)>([\s\S]*?)<\/class>/g)) {
    const name = attr(classAttrs, 'filename');
    if (!name) continue;
    // A relative file name is relative to one of the <source> folders; keep the first that exists.
    const path = isAbsolute(name) ? name : sources.map((s) => join(s, name)).find((p) => existsSync(p)) ?? name;
    const file = entry(files, path);
    for (const [, attrs] of body.matchAll(/<line\b([^>]*?)\/?>/g)) {
      const n = Number(attr(attrs, 'number'));
      addLine(file, n, Number(attr(attrs, 'hits') ?? 0));
      const cond = /\((\d+)\/(\d+)\)/.exec(attr(attrs, 'condition-coverage') ?? '');
      if (attr(attrs, 'branch') === 'true' && cond) addBranches(file, n, Number(cond[2]), Number(cond[1]));
    }
  }
  return files;
}

function parseJacoco(text) {
  const files = new Map();
  for (const [, pkgAttrs, pkgBody] of text.matchAll(/<package\b([^>]*)>([\s\S]*?)<\/package>/g)) {
    const dir = attr(pkgAttrs, 'name') ?? '';
    for (const [, srcAttrs, srcBody] of pkgBody.matchAll(/<sourcefile\b([^>]*)>([\s\S]*?)<\/sourcefile>/g)) {
      const file = entry(files, [dir, attr(srcAttrs, 'name')].filter(Boolean).join('/'));
      for (const [, attrs] of srcBody.matchAll(/<line\b([^>]*?)\/?>/g)) {
        const n = Number(attr(attrs, 'nr'));
        // A line counts as reached when any of its instructions ran (ci, covered instructions).
        addLine(file, n, Number(attr(attrs, 'ci') ?? 0));
        const missed = Number(attr(attrs, 'mb') ?? 0);
        const covered = Number(attr(attrs, 'cb') ?? 0);
        if (missed + covered > 0) addBranches(file, n, missed + covered, covered);
      }
    }
  }
  return files;
}

function parseGo(text) {
  const files = new Map();
  for (const line of text.split(/\r?\n/)) {
    // file.go:startLine.startCol,endLine.endCol statements count (the mode: line first matches no block)
    const m = /^(.+):(\d+)\.\d+,(\d+)\.\d+ \d+ (\d+)$/.exec(line.trim());
    if (!m) continue;
    const file = entry(files, m[1]);
    for (let n = Number(m[2]); n <= Number(m[3]); n++) addLine(file, n, Number(m[4]));
  }
  return files;
}

function detect(text) {
  if (/^mode: (set|count|atomic)/.test(text)) return ['Go cover profile', parseGo];
  if (/^\s*(TN:|SF:)/m.test(text)) return ['lcov', parseLcov];
  if (/<coverage\b/.test(text)) return ['Cobertura XML', parseCobertura];
  if (/<report\b/.test(text)) return ['JaCoCo XML', parseJacoco];
  return null;
}

/** "3-5, 9, 12-14" from a sorted list of line numbers. */
function ranges(numbers) {
  const out = [];
  for (let i = 0; i < numbers.length; ) {
    let j = i;
    while (numbers[j + 1] === numbers[j] + 1) j++; // past the end, undefined stops it
    out.push(i === j ? `${numbers[i]}` : `${numbers[i]}-${numbers[j]}`);
    i = j + 1;
  }
  return out.join(', ');
}

const pct = (hit, total) => (total ? `${((100 * hit) / total).toFixed(1)}%` : '-');

const TEST_FILE = /(^|[\\/])(tests?|__tests__|spec|e2e)[\\/]|[._-](test|spec)\.[^.\\/]+$|(^|[\\/])test_[^\\/]+$|_test\.[^.\\/]+$/i;
const SKIP_DIRS = new Set(['node_modules', '.git', 'target', 'dist', 'build', 'out', 'coverage', '.venv', 'venv', '__pycache__', 'vendor', '.next', 'bin', 'obj']);

function sourceFiles(dir, exts) {
  const out = [];
  const walk = (d) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name);
      if (statSync(p).isDirectory()) {
        if (!SKIP_DIRS.has(name) && !name.startsWith('.')) walk(p);
      } else if (exts.includes(extname(name).toLowerCase()) && !TEST_FILE.test(relative(dir, p)) && !/\.d\.ts$/.test(name)) out.push(p);
    }
  };
  walk(dir);
  return out;
}

// Python's trace (`python -m trace --count --missing --coverdir=<folder> ...`) writes <folder>/<module>.cover per
// module: a line that ran starts with its count ("    3: "), one that never ran with ">>>>>> ". Lines only. Modules
// that are not files under the root (the standard library, installed packages) are left out.
function parseTraceFolder(folder, root) {
  const files = new Map();
  for (const name of readdirSync(folder).filter((n) => n.endsWith('.cover'))) {
    const path = `${name.slice(0, -'.cover'.length).split('.').join('/')}.py`; // pkg.mod.cover is pkg/mod.py
    if (!existsSync(resolve(root, path))) continue;
    const file = entry(files, path);
    readFileSync(join(folder, name), 'utf8').split(/\r?\n/).forEach((line, i) => {
      const count = /^ {0,4}(\d+): /.exec(line);
      if (count) addLine(file, i + 1, Number(count[1]));
      else if (line.startsWith('>>>>>> ')) addLine(file, i + 1, 0);
    });
  }
  return files;
}

function readReport(args) {
  if (statSync(args.report).isDirectory()) return ['Python trace folder', parseTraceFolder(args.report, args.root)];
  const text = readFileSync(args.report, 'utf8');
  const found = detect(text);
  if (!found) fail(`${args.report} is not lcov, Cobertura XML, JaCoCo XML, a Go cover profile or a Python trace folder; export one of those (see reference/tools.md)`);
  const [format, parse] = found;
  const files = parse(text);
  // Every lcov record ends with end_of_record: a report that does not was cut off while being written.
  if (format === 'lcov' && files.size > 0 && !/end_of_record\s*$/.test(text)) fail(`${args.report} ends inside a record: it was cut off while being written; run the tests again`);
  return [format, files];
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!existsSync(args.report)) fail(`${args.report} does not exist`);
  const [format, files] = readReport(args);
  // With no file in the report there is nothing to rank; with --expect, every source file is listed as not loaded.
  if (files.size === 0 && !args.expect) fail(`${args.report} (${format}) names no source files`);

  const key = (p) => {
    const abs = isAbsolute(p) ? p : resolve(args.root, p);
    return process.platform === 'win32' ? abs.toLowerCase() : abs;
  };
  const show = (p) => {
    const rel = relative(args.root, isAbsolute(p) ? p : resolve(args.root, p));
    return (rel.startsWith('..') ? p : rel).split(sep).join('/');
  };

  const rows = [];
  let [lineHit, lineTotal, branchHit, branchTotal] = [0, 0, 0, 0];
  for (const [path, f] of files) {
    const lines = [...f.lines.entries()];
    const missedLines = lines.filter(([, h]) => h === 0).map(([n]) => n).sort((a, b) => a - b);
    const branches = [...f.branches.values()];
    const bFound = branches.reduce((s, b) => s + b.found, 0);
    const bTaken = branches.reduce((s, b) => s + b.taken, 0);
    lineHit += lines.length - missedLines.length;
    lineTotal += lines.length;
    branchHit += bTaken;
    branchTotal += bFound;
    const partial = [...f.branches.entries()].filter(([, b]) => b.taken < b.found).map(([n]) => n).sort((a, b) => a - b);
    rows.push({ path, lines: [lines.length - missedLines.length, lines.length], branches: [bTaken, bFound], missedLines, partial });
  }
  rows.sort((a, b) => b.branches[1] - b.branches[0] - (a.branches[1] - a.branches[0]) || b.missedLines.length - a.missedLines.length);

  console.log(`Coverage report ${args.report} (${format}): ${files.size} file(s)`);
  const gaps = rows.filter((r) => r.missedLines.length || r.branches[0] < r.branches[1]);
  if (files.size === 0) console.log('No test loaded any source file.');
  else {
    console.log(`Lines: ${lineHit} of ${lineTotal} (${pct(lineHit, lineTotal)})`);
    console.log(branchTotal ? `Branches: ${branchHit} of ${branchTotal} (${pct(branchHit, branchTotal)})` : 'Branches: none in this report; measure with branches (reference/tools.md) before calling a file covered');
    console.log('');
  }
  if (files.size > 0 && gaps.length === 0) console.log('Every line and branch in the report was reached.');
  else if (gaps.length > 0) {
    console.log(`Files with gaps, most missed branches first (top ${Math.min(args.top, gaps.length)} of ${gaps.length}):`);
    console.log('| File | Lines | Branches | Lines never run | Lines with a branch not taken |');
    console.log('|---|---|---|---|---|');
    for (const r of gaps.slice(0, args.top)) {
      console.log(`| ${show(r.path)} | ${r.lines[0]}/${r.lines[1]} (${pct(...r.lines)}) | ${r.branches[1] ? `${r.branches[0]}/${r.branches[1]} (${pct(...r.branches)})` : '-'} | ${ranges(r.missedLines) || '-'} | ${ranges(r.partial) || '-'} |`);
    }
  }

  if (args.expect) {
    const dir = resolve(args.root, args.expect);
    if (!existsSync(dir)) fail(`--expect ${args.expect} does not exist`);
    const exts = args.ext ?? [...new Set([...files.keys()].map((p) => extname(p).toLowerCase()).filter(Boolean))];
    if (exts.length === 0) fail(`the report names no file to take the extensions from; give them with --ext, such as --ext .ts`);
    const reported = new Set([...files.keys()].map(key));
    const missing = sourceFiles(dir, exts).filter((p) => !reported.has(key(p)));
    console.log('');
    if (missing.length === 0) console.log(`Every ${exts.join(' ')} source file under ${args.expect} is in the report.`);
    else {
      console.log(`Not in the report at all (no test loaded them; count them as 0%): ${missing.length}`);
      for (const p of missing) console.log(`  ${show(p)}`);
    }
  }
}

main();
