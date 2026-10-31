#!/usr/bin/env node
// Quick mutation testing for any language: changes one operator or literal at a time in one source file, runs the
// project's test command, puts the file back, and lists the mutants no test noticed ("survivors"). A survivor is a
// change of behaviour that no test checks: add a test that fails on it, or say why it cannot change behaviour.
//
// Usage:
//   node mutate.mjs --file <source> --test "<command>" [--lines 10-80] [--check "<command>"] [--max 60]
//                   [--timeout <seconds>] [--allow-dirty] [--list]
//   node mutate.mjs --restore --file <source>      puts the file back after a run that was killed
//
// It is textual, not syntax-aware: it skips comments and string literals, and touches `<` `>` `+` `-` `*` and the
// boolean words only with spaces around them, so generics (`Vec<u8>`), arrows (`=>`, `->`) and unary minus stay. A
// mutant that breaks the syntax makes the tests fail and would count as caught; `--check` (a compile or type check)
// sorts those out as unviable. For a .py file it removes the file's __pycache__ bytecode after every write and runs
// the commands with PYTHONDONTWRITEBYTECODE=1, so no run reads an older version's bytecode. For a thorough run use the
// ecosystem's own tool (Stryker, mutmut, cargo-mutants, PIT).
// Needs Node 18 or later. Exit code: 0 no survivors, 1 survivors, 2 the run could not be done.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join, resolve } from 'node:path';

// Default ceiling per test run when --timeout is not given: five times the unmutated run, and never under 10 s, so a
// slow machine does not turn ordinary runs into timeouts.
const TIMEOUT_FACTOR = 5;
const TIMEOUT_FLOOR_MS = 10_000;

function fail(message) {
  console.error(`mutate: ${message}`);
  process.exit(2);
}

function parseArgs(argv) {
  const args = { lines: null, max: Infinity, timeout: null, allowDirty: false, list: false, restore: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) fail(`${a} needs a value`);
      return argv[++i];
    };
    if (a === '--file') args.file = value();
    else if (a === '--test') args.test = value();
    else if (a === '--check') args.check = value();
    else if (a === '--lines') {
      const m = /^(\d+)(?:-(\d+))?$/.exec(value());
      if (!m) fail('--lines takes a line or a range, such as 12 or 10-80');
      args.lines = [Number(m[1]), Number(m[2] ?? m[1])];
    } else if (a === '--max') {
      args.max = Number(value());
      if (!Number.isInteger(args.max) || args.max < 1) fail('--max takes a whole number of at least 1');
    } else if (a === '--timeout') {
      const seconds = Number(value());
      if (!(seconds > 0)) fail('--timeout takes a number of seconds above 0');
      args.timeout = seconds * 1000;
    } else if (a === '--allow-dirty') args.allowDirty = true;
    else if (a === '--list') args.list = true;
    else if (a === '--restore') args.restore = true;
    else if (a === '--help' || a === '-h') {
      const header = readFileSync(new URL(import.meta.url), 'utf8').split(/\r?\n/).slice(1);
      console.log(header.slice(0, header.findIndex((l) => !l.startsWith('//'))).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
      process.exit(0);
    } else fail(`unknown option ${a}; see --help`);
  }
  if (!args.file) fail('--file <source> is required');
  if (!args.restore && !args.list && !args.test) fail('--test "<command>" is required (the command that runs the tests)');
  return args;
}

// Comment and string syntax by file extension. Unknown extensions get the C-like rules. `conditions` is how an `if`
// or `while` condition is written: Python's `if x:`, C-like `if (x)`, or `if x {` in Rust, Go and Swift (none: the
// language is not handled).
const HASH_COMMENT = new Set(['.py', '.rb', '.sh', '.bash', '.pl', '.r', '.toml', '.yaml', '.yml', '.ex', '.exs', '.nim']);
const DASH_COMMENT = new Set(['.sql', '.lua', '.hs', '.elm']);
const BRACE_CONDITIONS = new Set(['.rs', '.go', '.swift']);

function syntaxOf(file) {
  const ext = extname(file).toLowerCase();
  if (HASH_COMMENT.has(ext)) return { line: '#', block: null, triple: ext === '.py', rustChars: false, conditions: ext === '.py' ? 'python' : null };
  if (DASH_COMMENT.has(ext)) return { line: '--', block: null, triple: false, rustChars: false, conditions: null };
  return { line: '//', block: ['/*', '*/'], triple: false, rustChars: ext === '.rs', conditions: BRACE_CONDITIONS.has(ext) ? 'brace' : 'paren' };
}

/**
 * Each line with its strings and comments blanked out (same length), so operators are found only in code, and per
 * line the [start, end) spans of what one-line quoted strings hold. `open` carries a block comment, a triple-quoted
 * string or a template literal from one line to the next.
 */
function maskLines(lines, syntax) {
  const masked = [];
  const strings = [];
  let open = null; // '*/', '"""', "'''" or '`' while a construct continues across lines
  for (const line of lines) {
    let out = '';
    const spans = [];
    let i = 0;
    while (i < line.length) {
      if (open) {
        const end = open === '`' ? findTemplateEnd(line, i) : line.indexOf(open, i);
        if (end < 0) {
          out += ' '.repeat(line.length - i);
          i = line.length;
        } else {
          const stop = end + (open === '`' ? 1 : open.length);
          out += ' '.repeat(stop - i);
          i = stop;
          open = null;
        }
        continue;
      }
      const rest = line.slice(i);
      if (rest.startsWith(syntax.line)) {
        out += ' '.repeat(line.length - i);
        break;
      }
      if (syntax.block && rest.startsWith(syntax.block[0])) {
        open = syntax.block[1];
        out += '  ';
        i += 2;
        continue;
      }
      if (syntax.triple && (rest.startsWith('"""') || rest.startsWith("'''"))) {
        open = rest.slice(0, 3);
        out += '   ';
        i += 3;
        continue;
      }
      const c = line[i];
      if (c === '`' && !syntax.triple) {
        open = '`';
        out += ' ';
        i += 1;
        continue;
      }
      if (c === '"' || c === "'") {
        // In Rust a quote opens a string only as a character literal ('x', '\n'); otherwise it is a lifetime ('a).
        if (c === "'" && syntax.rustChars && !/^'(\\.[^']*|[^'\\])'/.test(rest)) {
          out += c;
          i += 1;
          continue;
        }
        let j = i + 1;
        while (j < line.length && line[j] !== c) j += line[j] === '\\' ? 2 : 1;
        if (j < line.length) spans.push([i + 1, j]);
        const stop = Math.min(j + 1, line.length);
        out += ' '.repeat(stop - i);
        i = stop;
        continue;
      }
      out += c;
      i += 1;
    }
    masked.push(out);
    strings.push(spans);
  }
  return { masked, strings };
}

function findTemplateEnd(line, from) {
  for (let j = from; j < line.length; j++) {
    if (line[j] === '\\') j++;
    else if (line[j] === '`') return j;
  }
  return -1;
}

// What a mutant changes, longest first so `===` is never also taken as `==`. Spaces are part of the patterns that
// need them (see the header).
const RULES = [
  ['===', '!=='],
  ['!==', '==='],
  [' += ', ' -= '],
  [' -= ', ' += '],
  [' <= ', ' < '],
  [' >= ', ' > '],
  [' && ', ' || '],
  [' || ', ' && '],
  [' and ', ' or '],
  ['==', '!='],
  ['!=', '=='],
  [' < ', ' <= '],
  [' > ', ' >= '],
  [' + ', ' - '],
  [' - ', ' + '],
  [' * ', ' / '],
  [' or ', ' and '],
];
const WORDS = [
  ['true', 'false'],
  ['false', 'true'],
  ['True', 'False'],
  ['False', 'True'],
];
// A skipped line: imports, attributes, decorators and preprocessor lines hold no behaviour to mutate.
const SKIP_LINE = /^\s*(import\s|from\s+\S+\s+import\s|use\s|#\[|#!|@|#include|#define|package\s|using\s)/;

/** A number one unit up and one down in its last digit: 10 gives 11 and 9, 0.15 gives 0.16 and 0.14 (none below 0). */
function neighbours(literal) {
  const m = /^(-?)(\d[\d_]*)(?:\.(\d+))?$/.exec(literal);
  if (!m) return [];
  const [, sign, whole, fraction = ''] = m;
  const units = Number(whole.replace(/_/g, '') + fraction);
  if (!Number.isSafeInteger(units)) return [];
  const show = (u) => {
    const digits = String(u).padStart(fraction.length + 1, '0');
    return sign + (fraction ? `${digits.slice(0, -fraction.length)}.${digits.slice(-fraction.length)}` : digits);
  };
  return units > 0 ? [show(units + 1), show(units - 1)] : [show(units + 1)];
}

/** Where the `if` or `while` condition of a masked line starts and ends, when it ends on that line; else null. */
function conditionSpan(code, style) {
  if (style === 'python') {
    const m = /^\s*(?:if|elif|while)\s+/.exec(code);
    if (!m) return null;
    let depth = 0;
    for (let j = m[0].length; j < code.length; j++) {
      if ('([{'.includes(code[j])) depth++;
      else if (')]}'.includes(code[j])) depth--;
      else if (code[j] === ':' && depth === 0 && code[j + 1] !== '=') return [m[0].length, j];
    }
    return null;
  }
  if (style === 'paren') {
    const m = /(?<![\w$.])(?:if|while)\s*\(/.exec(code);
    if (!m) return null;
    let depth = 0;
    for (let j = m.index + m[0].length - 1; j < code.length; j++) {
      if (code[j] === '(') depth++;
      else if (code[j] === ')' && --depth === 0) return [m.index + m[0].length, j];
    }
    return null;
  }
  if (style === 'brace') {
    // Up to the block's `{` at the end of the line; in Go after the `;` that ends an init statement (`if v, ok := m[k]; ok {`).
    const m = /(?<![\w$.])(?:if|while)\s+(?!let\b|var\b|case\b)/.exec(code);
    const end = code.search(/\{\s*$/);
    if (!m || end < m.index + m[0].length) return null;
    return [Math.max(m.index + m[0].length, code.lastIndexOf(';', end) + 1), end];
  }
  return null;
}

/** Every mutant of the file inside the line range, in order: { line, column, from, to, kind }. */
function findMutants(text, file, range) {
  const lines = text.split(/\r?\n/);
  const syntax = syntaxOf(file);
  const { masked, strings } = maskLines(lines, syntax);
  const mutants = [];
  for (let n = 0; n < lines.length; n++) {
    const lineNo = n + 1;
    if (range && (lineNo < range[0] || lineNo > range[1])) continue;
    if (SKIP_LINE.test(lines[n])) continue;
    const code = masked[n];
    for (let i = 0; i < code.length; ) {
      const rule = RULES.find(([from]) => code.startsWith(from, i));
      if (rule) {
        mutants.push({ line: lineNo, column: i, from: rule[0], to: rule[1], kind: 'operator' });
        i += rule[0].length - (rule[0].endsWith(' ') ? 1 : 0);
        continue;
      }
      // `!x` becomes `x`; not `!=`, and not a macro (`println!`) or a non-null assertion (`a!.b`).
      if (code[i] === '!' && code[i + 1] !== '=' && !/[\w)\]]/.test(code[i - 1] ?? '') && /[\w(!]/.test(code[i + 1] ?? '')) {
        mutants.push({ line: lineNo, column: i, from: '!', to: '', kind: 'negation' });
        i += 1;
        continue;
      }
      i += 1;
    }
    for (const [from, to] of WORDS) {
      for (const m of code.matchAll(new RegExp(`(?<![\\w$])${from}(?![\\w$])`, 'g'))) {
        mutants.push({ line: lineNo, column: m.index, from, to, kind: 'constant' });
      }
    }
    // Python's `not x` becomes `x`.
    for (const m of code.matchAll(/(?<![\w$])not (?!in\b)/g)) {
      mutants.push({ line: lineNo, column: m.index, from: 'not ', to: '', kind: 'negation' });
    }
    // Number literals, not parts of names, hex or exponents, and strings that hold only a number (Decimal("0.15"),
    // new BigDecimal("10.00"): rates and money are often written so): one up and one down in the last digit.
    const numbers = [...code.matchAll(/(?<![\w.$])\d[\d_]*(?:\.\d+)?(?![\w.$])/g)].map((m) => [m.index, m[0]]);
    for (const [start, end] of strings[n]) numbers.push([start, lines[n].slice(start, end)]);
    for (const [column, from] of numbers) {
      for (const to of neighbours(from)) mutants.push({ line: lineNo, column, from, to, kind: 'number' });
    }
    // The whole condition negated, when it ends on its line: `if (x)` becomes `if (!(x))`, Python's `if x:` becomes
    // `if not (x):`, `if x {` becomes `if !(x) {`. One that already starts with a negation is left to the mutants above.
    const span = conditionSpan(code, syntax.conditions);
    const condition = span ? lines[n].slice(...span).trim() : '';
    if (condition && !/^(!|not\b)/.test(condition)) {
      const column = span[0] + lines[n].slice(...span).indexOf(condition);
      mutants.push({ line: lineNo, column, from: condition, to: syntax.conditions === 'python' ? `not (${condition})` : `!(${condition})`, kind: 'condition' });
    }
  }
  return mutants.sort((a, b) => a.line - b.line || a.column - b.column);
}

/** The text with one mutant applied; line ends are kept as they were. */
function applyMutant(text, mutant) {
  const parts = text.split(/(\r?\n)/);
  const index = (mutant.line - 1) * 2;
  const line = parts[index];
  parts[index] = line.slice(0, mutant.column) + mutant.to + line.slice(mutant.column + mutant.from.length);
  return parts.join('');
}

/** Picks `max` mutants spread evenly over the list, so a cap still samples the whole range. */
function sample(mutants, max) {
  if (mutants.length <= max) return mutants;
  return Array.from({ length: max }, (_, i) => mutants[Math.floor((i * mutants.length) / max)]);
}

function killTree(child) {
  if (process.platform === 'win32') spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  else {
    try {
      process.kill(-child.pid, 'SIGKILL');
    } catch {
      child.kill('SIGKILL');
    }
  }
}

/** Runs a shell command; resolves to { code, timedOut, ms, output } (output: the last part, for messages). */
function run(command, cwd, timeoutMs, extraEnv) {
  return new Promise((resolvePromise) => {
    const started = Date.now();
    // Without NODE_TEST_CONTEXT: under a Node test runner the variable makes a nested `node --test` report to the
    // outer runner and exit 0 whatever happened.
    const { NODE_TEST_CONTEXT: _nested, ...env } = process.env;
    Object.assign(env, extraEnv);
    const child = spawn(command, { cwd, env, shell: true, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    const keep = (chunk) => {
      output = (output + chunk).slice(-4000);
    };
    child.stdout.on('data', keep);
    child.stderr.on('data', keep);
    let timedOut = false;
    const timer = timeoutMs
      ? setTimeout(() => {
          timedOut = true;
          killTree(child);
        }, timeoutMs)
      : null;
    child.on('close', (code) => {
      if (timer) clearTimeout(timer);
      resolvePromise({ code, timedOut, ms: Date.now() - started, output });
    });
    child.on('error', (e) => {
      if (timer) clearTimeout(timer);
      resolvePromise({ code: -1, timedOut: false, ms: Date.now() - started, output: String(e) });
    });
  });
}

/** Where a run keeps the original file until it ends, so a killed run can be undone with --restore. */
function guardPaths(file) {
  const id = createHash('sha1').update(file.toLowerCase()).digest('hex').slice(0, 16);
  const dir = join(tmpdir(), 'mutate-guard');
  return { dir, record: join(dir, `${id}.json`), backup: join(dir, `${id}.orig`) };
}

function restoreFromGuard(file) {
  const guard = guardPaths(file);
  if (!existsSync(guard.record)) {
    console.log(`mutate: nothing to restore for ${file}`);
    return;
  }
  put(file, readFileSync(guard.backup));
  rmSync(guard.record, { force: true });
  rmSync(guard.backup, { force: true });
  console.log(`mutate: ${file} is back as it was before the run`);
}

function gitDirty(file) {
  const r = spawnSync('git', ['status', '--porcelain', '--', basename(file)], { cwd: dirname(file), encoding: 'utf8' });
  if (r.status !== 0) return false; // not a git repository, or no git: the backup alone protects the file
  return r.stdout.trim() !== '';
}

const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

// CPython reuses __pycache__/<name>.*.pyc while the source keeps its size and its mtime in whole seconds. Most mutants
// keep the size, and runs are often under a second apart, so a mutant would run the bytecode of the version before
// it (and the last mutant's would shadow the restored file). The cache of the file is removed after every write.
function dropBytecode(file) {
  const cache = join(dirname(file), '__pycache__');
  if (extname(file).toLowerCase() !== '.py' || !existsSync(cache)) return;
  const prefix = `${basename(file, extname(file))}.`;
  for (const name of readdirSync(cache)) if (name.startsWith(prefix) && name.endsWith('.pyc')) rmSync(join(cache, name), { force: true });
}

/** Writes the file and removes what could keep its earlier version alive. */
function put(file, bytes) {
  writeFileSync(file, bytes);
  dropBytecode(file);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const file = resolve(args.file);
  if (!existsSync(file)) fail(`${args.file} does not exist`);
  if (args.restore) return restoreFromGuard(file);

  const original = readFileSync(file);
  const text = original.toString('utf8');
  const all = findMutants(text, file, args.lines);
  const mutants = sample(all, args.max);
  // file:line:column of the changed token (past the space a spaced operator starts with), so two mutants of the same
  // operator on one line can be told apart.
  const where = (m) => `${args.file}:${m.line}:${m.column + 1 + (m.from.startsWith(' ') ? 1 : 0)}`;
  const describe = (m) => `${JSON.stringify(m.from)} -> ${JSON.stringify(m.to)}`;
  const lineOf = (m) => text.split(/\r?\n/)[m.line - 1].trim().slice(0, 100);

  if (args.list) {
    for (const m of mutants) console.log(`${where(m)}  ${m.kind}  ${describe(m)}    ${lineOf(m)}`);
    console.log(`${mutants.length} mutant(s)${all.length > mutants.length ? ` (sampled from ${all.length})` : ''}`);
    return;
  }
  if (mutants.length === 0) fail('no mutants in that file or range; check --lines');

  const guard = guardPaths(file);
  if (existsSync(guard.record)) {
    fail(`an earlier run on ${args.file} did not finish and its original is saved; put it back first with:\n  node ${process.argv[1]} --restore --file ${args.file}`);
  }
  if (!args.allowDirty && gitDirty(file)) {
    fail(`${args.file} has uncommitted changes; commit or stash them first (so git can always restore the file), or pass --allow-dirty`);
  }
  const cwd = process.cwd();
  const python = extname(file).toLowerCase() === '.py';
  const env = python ? { PYTHONDONTWRITEBYTECODE: '1' } : {};

  console.log(`Running the tests once on the unchanged file: ${args.test}`);
  const first = await run(args.test, cwd, null, env);
  if (first.code !== 0) fail(`the tests fail before any mutation (exit ${first.code}); fix them first. The end of their output:\n${first.output}`);
  const timeout = args.timeout ?? Math.max(TIMEOUT_FLOOR_MS, first.ms * TIMEOUT_FACTOR);
  console.log(`They pass in ${(first.ms / 1000).toFixed(1)} s; each mutant gets ${(timeout / 1000).toFixed(0)} s.`);

  mkdirSync(guard.dir, { recursive: true });
  writeFileSync(guard.backup, original);
  writeFileSync(guard.record, JSON.stringify({ file, started: new Date().toISOString() }));
  let restored = false;
  const restore = () => {
    if (restored) return;
    put(file, original);
    rmSync(guard.record, { force: true });
    rmSync(guard.backup, { force: true });
    restored = true;
  };
  process.on('exit', restore);
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => {
      restore();
      process.exit(130);
    });
  }

  const results = [];
  try {
    for (const [i, m] of mutants.entries()) {
      put(file, applyMutant(text, m));
      let status;
      if (args.check && (await run(args.check, cwd, timeout, env)).code !== 0) status = 'unviable';
      else {
        const r = await run(args.test, cwd, timeout, env);
        status = r.timedOut ? 'timeout' : r.code === 0 ? 'missed' : 'caught';
      }
      put(file, original);
      results.push({ ...m, status });
      console.log(`[${i + 1}/${mutants.length}] ${where(m)} ${describe(m)}: ${status}`);
    }
  } finally {
    restore();
  }
  if (hash(readFileSync(file)) !== hash(original)) fail(`${args.file} differs from before the run; restore it from git`);

  const count = (s) => results.filter((r) => r.status === s).length;
  const [caught, missed, timedOut, unviable] = ['caught', 'missed', 'timeout', 'unviable'].map(count);
  console.log('');
  console.log(`Mutants: ${results.length}${all.length > results.length ? ` (sampled from ${all.length})` : ''}: caught ${caught}, missed ${missed}, timeout ${timedOut}, unviable ${unviable}`);
  if (caught + missed > 0) console.log(`Score: ${caught} of ${caught + missed} caught (${((100 * caught) / (caught + missed)).toFixed(0)}%), timeouts and unviable ones left out`);
  if (!args.check) console.log('No --check given: a mutant that breaks the syntax counts as caught.');
  const show = (status, heading) => {
    const list = results.filter((r) => r.status === status);
    if (!list.length) return;
    console.log(heading);
    for (const m of list) console.log(`  ${where(m)}  ${describe(m)}    ${lineOf(m)}`);
  };
  show('missed', 'Survivors (a test should fail on each, unless the change cannot alter behaviour):');
  show('timeout', 'Timeouts (the tests did not finish: an endless loop in the code or in a test; look at each):');
  process.exitCode = missed > 0 ? 1 : 0;
}

main().catch((e) => fail(e.stack ?? String(e)));
