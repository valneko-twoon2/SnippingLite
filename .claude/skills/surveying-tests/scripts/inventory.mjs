#!/usr/bin/env node
// A first look at how a repository is tested: its languages, how its tests are run (manifest scripts, CI), its test
// files, the signals of each kind of test (in dependencies, configuration, file names and test code), and the largest
// source files with no test in sight. It reads names, manifests and test files, not the logic, so a "found" is a lead
// to confirm by opening the files it names, and "none found" means only that no known signal is there.
//
// Usage: node inventory.mjs [repository folder]      (default: the current folder)
// Lists files with `git ls-files` when the folder is in a git repository (ignored files stay out), else walks it.
// Needs Node 18 or later.
import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, extname, join, resolve } from 'node:path';

// Reading limits, so a huge repository still answers in seconds: files above this size are not read for signals.
const MAX_READ_BYTES = 300_000;
const MAX_READ_FILES = 4000;
// Folders of dependencies, builds, caches and tools; `.claude` holds this skill itself when it is installed in the
// project, and its scripts are not the project's tests.
const SKIP_DIRS = new Set(['node_modules', '.git', 'target', 'dist', 'build', 'out', 'coverage', '.venv', 'venv', '__pycache__', '.tox', 'vendor', '.next', '.nuxt', 'bin', 'obj', '.gradle', '.idea', '.mypy_cache', '.pytest_cache', '.claude']);

const LANGUAGES = {
  '.ts': 'TypeScript', '.tsx': 'TypeScript', '.mts': 'TypeScript', '.cts': 'TypeScript',
  '.js': 'JavaScript', '.jsx': 'JavaScript', '.mjs': 'JavaScript', '.cjs': 'JavaScript',
  '.py': 'Python', '.rs': 'Rust', '.go': 'Go', '.java': 'Java', '.kt': 'Kotlin', '.kts': 'Kotlin',
  '.cs': 'C#', '.fs': 'F#', '.rb': 'Ruby', '.php': 'PHP', '.swift': 'Swift', '.c': 'C', '.h': 'C', '.cpp': 'C++',
  '.cc': 'C++', '.hpp': 'C++', '.scala': 'Scala', '.dart': 'Dart', '.ex': 'Elixir', '.exs': 'Elixir', '.lua': 'Lua',
  '.vue': 'Vue', '.svelte': 'Svelte',
};

const TEST_PATH = /(^|\/)(tests?|__tests__|spec|specs|e2e|integration[-_]tests?)\/|[._-](test|spec|tests)\.[^./]+$|(^|\/)tests?\.rs$|(^|\/)test_[^/]+\.py$|_test\.(go|py|rs|exs?)$|(Test|Tests|Spec|IT)\.(java|kt|cs|scala)$/;

const MANIFESTS = /(^|\/)(package\.json|Cargo\.toml|pyproject\.toml|setup\.cfg|setup\.py|requirements[^/]*\.txt|Pipfile|tox\.ini|pytest\.ini|noxfile\.py|go\.mod|pom\.xml|build\.gradle(\.kts)?|[^/]+\.csproj|Directory\.Packages\.props|Gemfile|composer\.json|deno\.jsonc?|mutants\.toml|stryker\.[^/]+|\.coveragerc|\.nycrc[^/]*|\.c8rc[^/]*|vitest\.config\.[^/]+|jest\.config\.[^/]+|playwright\.config\.[^/]+|cypress\.config\.[^/]+|codecov\.ya?ml|deny\.toml|renovate\.json5?)$/;
// The manifests that declare dependencies; the rest of MANIFESTS is configuration and is read as configuration only.
const DEPENDENCY_MANIFESTS = /(^|\/)(package\.json|Cargo\.toml|pyproject\.toml|setup\.cfg|setup\.py|requirements[^/]*\.txt|Pipfile|go\.mod|pom\.xml|build\.gradle(\.kts)?|[^/]+\.csproj|Directory\.Packages\.props|Gemfile|composer\.json|deno\.jsonc?)$/;
const CI_FILES = /(^|\/)(\.github\/workflows\/[^/]+\.ya?ml|\.gitlab-ci\.ya?ml|azure-pipelines\.ya?ml|Jenkinsfile|\.circleci\/config\.ya?ml|bitbucket-pipelines\.ya?ml|\.buildkite\/[^/]+|Makefile|justfile|Taskfile\.ya?ml)$/;

/**
 * The kinds of test and their signals: `deps` is matched against the declared dependencies, `files` against every path
 * (only test paths and scripts when `testOnly`: those words also name ordinary code), `code` against the test files'
 * text, `config` against manifests and CI files (commands, settings).
 */
const KINDS = [
  { kind: 'coverage measurement', deps: /["'\s](c8|nyc|istanbul|@vitest\/coverage-[\w-]+|pytest-cov|coverage|cargo-llvm-cov|cargo-tarpaulin|jacoco|coverlet[\w.]*|simplecov|kover)["'\s=:<>~^]/i, files: /(^|\/)(\.coveragerc|\.nycrc[^/]*|\.c8rc[^/]*|codecov\.ya?ml|\.simplecov)$/, config: /--coverage|--experimental-test-coverage|llvm-cov|tarpaulin|coverprofile|-cover\b|XPlat Code Coverage|coverage run|jacocoTestReport/ },
  { kind: 'mutation testing', deps: /@stryker-mutator|["'\s]mutmut["'\s=]|cosmic-ray|cargo-mutants|pitest|stryker/i, files: /(^|\/)(stryker\.(conf|config)\.[^/]+|stryker-config\.json|\.gremlins\.ya?ml|mutants\.toml)$/, config: /cargo mutants|mutmut run|pitest|stryker run|gremlins unleash|\[mutmut\]|\[tool\.mutmut\]/ },
  { kind: 'property-based', deps: /["'\s](fast-check|jsverify|hypothesis|proptest|quickcheck|jqwik|FsCheck|CsCheck|gopter|pgregory\.net\/rapid|rantly|propcheck)["'\s=:<>~^\/]/i, code: /\bfc\.(assert|property|asyncProperty)\b|@given\(|proptest!|quickcheck!|#\[quickcheck\]|testing\/quick|rapid\.Check|@Property\b|Prop\.ForAll/ },
  { kind: 'model-based (stateful)', deps: /proptest-state-machine/, code: /fc\.(commands|modelRun|asyncModelRun)|RuleBasedStateMachine|prop_state_machine!|StateMachineTest/ },
  { kind: 'fuzzing', deps: /["'\s](atheris|libfuzzer-sys|cargo-fuzz|afl|honggfuzz|jazzer|@jazzer\.js\/core|SharpFuzz|jsfuzz)["'\s=:<>~^]/i, files: /(^|\/)fuzz(ing)?\/|(^|\/)[^/]*fuzz[^/]*\.(rs|py|go|ts|js|java|kt|cs|c|cc|cpp)$|(^|\/)testdata\/fuzz\//i, code: /fuzz_target!|func Fuzz\w*\(|LLVMFuzzerTestOneInput|atheris\.Setup|@FuzzTest|\bfuzz(ed|ing)?\b/i },
  { kind: 'snapshot / golden', deps: /["'\s](insta|syrupy|snapshottest|pytest-snapshot|Verify\.\w+|approvaltests)["'\s=:<>~^]/i, files: /__snapshots__\/|\.snap$|(^|\/)(golden|goldens|testdata\/golden)\/|\.approved\./, code: /toMatchSnapshot|toMatchInlineSnapshot|toMatchFileSnapshot|insta::assert|assert_snapshot!|snapshot\.assert_match|assert_match_snapshot|Verifier\.Verify/ },
  { kind: 'contract', deps: /@pact-foundation\/pact|pact-python|pact_consumer|pact-jvm|PactNet|schemathesis|dredd|spring-cloud-contract/i, files: /(^|\/)(contracts?|pacts?)\/[^/]+\.(json|ya?ml)$/ },
  { kind: 'end-to-end (browser or app)', deps: /@playwright\/test|["'\s]playwright["'\s=:<>~^]|cypress|selenium|webdriverio|@wdio\/|puppeteer|nightwatch|testcafe|detox|appium/i, files: /(^|\/)(playwright|cypress|wdio|nightwatch)\.conf(ig)?\.[^/]+$|(^|\/)(e2e|cypress)\// },
  { kind: 'cross-browser / device', config: /devices\[['"](iPhone|iPad|Pixel|Galaxy)|browserName:\s*['"](webkit|firefox)|name:\s*['"](webkit|firefox|Mobile Safari|Mobile Chrome)['"]/i },
  { kind: 'visual regression', deps: /percy|chromatic|backstopjs|loki|jest-image-snapshot|reg-suit|argos/i, code: /toHaveScreenshot|toMatchImageSnapshot|matchImageSnapshot|percySnapshot|compareSnapshot|screenshot\(\)\.to/ },
  { kind: 'accessibility', deps: /axe-core|@axe-core\/|pa11y|jest-axe|cypress-axe|vitest-axe/i, code: /AxeBuilder|checkA11y|toHaveNoViolations|axe\.run/ },
  { kind: 'differential', testOnly: true, files: /diff(erential)?[-_.]?tests?|difftest/i, code: /\bdifferential\b|same (answers?|results?) (as|from)|reference implementation/i },
  { kind: 'monkey / random UI actions', testOnly: true, deps: /gremlins\.js|["'\s]monkey/i, files: /monkey/i, code: /\bmonkey\b|random (clicks|actions)/i },
  { kind: 'crash consistency / fault injection', testOnly: true, deps: /failpoint|fail-rs|["'\s]fail["'\s=]|toxiproxy|chaos-?monkey|pumba/i, files: /crash|power[-_]?loss|fault[-_]?inject|chaos/i, code: /fail_point!|fail::cfg|\bcrash(ed|es)? (after|during|while)|cut off after|kill(ed)? (during|mid)/i },
  { kind: 'upgrade / old data', files: /(^|\/)(fixtures?|testdata|test-data)\/[^/]*(v\d+[^/]*|\d+\.\d+\.\d+|old|legacy|previous)[^/]*\/|migrations?[^/]*\.(test|spec)\.|(^|\/)[^/]*(compat|upgrade|migration)[^/]*_test\.|test_[^/]*(compat|upgrade|migration)/i, code: /\b(old|older|previous) (version|release|data folder)|written by (v|version )\d/i },
  { kind: 'release smoke', testOnly: true, files: /smoke/i, config: /\bsmoke\b/i },
  { kind: 'performance / benchmarks', deps: /criterion|divan|pytest-benchmark|asv|tinybench|benchmark\.js|BenchmarkDotNet|jmh|["'\s]k6["'\s]|locust|artillery|hyperfine|mitata/i, files: /(^|\/)benches?\/|\.bench\.[^/]+$|(^|\/)benchmarks?\//i, code: /func Benchmark\w*\(|#\[bench\]|(?<![\w.])bench\(\s*['"`]|@pytest\.mark\.benchmark|criterion_group!/ },
  { kind: 'dependency / security audit', files: /(^|\/)(deny\.toml|\.github\/dependabot\.ya?ml|renovate\.json5?|\.gitleaks\.toml|\.semgrep(\.ya?ml)?|\.snyk|osv-scanner\.toml)$|codeql/i, config: /npm audit|pnpm audit|yarn audit|cargo audit|cargo deny|pip-audit|safety check|osv-scanner|trivy|snyk test|bandit|gosec|govulncheck|dotnet list package --vulnerable/ },
  { kind: 'retries that can hide flakiness', config: /retries\s*[:=]\s*[1-9]|retryTimes\(|--reruns|flaky|@Retry\b|retry\s*=\s*[1-9]|nextest[^\n]*retries/i, code: /retryTimes\(|@flaky|pytest\.mark\.flaky|this\.retries\(/ },
];

/** The files to read, how they were listed, and what was left out: { how, files, leftOut } (leftOut: a phrase or ''). */
function listFiles(root) {
  const git = spawnSync('git', ['ls-files', '-co', '--exclude-standard', '-z'], { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  if (git.status === 0) {
    const skipped = new Map(); // folder name -> files under it
    const files = git.stdout.split('\0').filter(Boolean).filter((f) => {
      const dir = f.split('/').find((p) => SKIP_DIRS.has(p));
      if (dir) skipped.set(dir, (skipped.get(dir) ?? 0) + 1);
      return !dir;
    });
    return { how: 'git ls-files', files, leftOut: [...skipped].map(([d, n]) => `${n} under ${d}`).join(', ') };
  }
  const files = [];
  const skipped = new Set();
  const walk = (dir, prefix) => {
    for (const name of readdirSync(dir)) {
      const full = join(dir, name);
      let st;
      try {
        st = statSync(full);
      } catch {
        continue;
      }
      if (st.isDirectory()) {
        if (!SKIP_DIRS.has(name)) walk(full, `${prefix}${name}/`);
        else skipped.add(name);
      } else files.push(`${prefix}${name}`);
    }
  };
  walk(root, '');
  return { how: 'a walk of the folder (not a git repository)', files, leftOut: skipped.size ? `the folders ${[...skipped].join(', ')}` : '' };
}

// Where a project without manifests or CI often says how to run its tests.
const DOC_FILES = /^(README|CONTRIBUTING|DEVELOPMENT|HACKING|TESTING)(\.[a-z]+)?$|^docs\/(development|contributing|testing)\.md$/i;
// A test command as documents write it: a test runner ("pytest -q"), or a known tool with a test word after it
// ("python -m unittest discover", "npm test").
const DOC_COMMAND = /^(\$\s*)?((pytest|tox|nox|rspec|phpunit|jest|vitest|mocha|ctest)\b|(npm|pnpm|yarn|npx|bun|deno|node|python3?|py|cargo|go|mvn|\.?\/?gradlew?|dotnet|make|just|bundle|rake|mix|composer)\b.*\b(test|tests|pytest|unittest|spec|check|coverage|jest|vitest|mocha|playwright)\b)/i;

/** The test commands in a Markdown or text document: lines of fenced code blocks, and inline `code` in the prose. */
function docCommands(text) {
  const found = new Set();
  let fenced = false;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (/^(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    const candidates = fenced ? [line] : [...line.matchAll(/`([^`]+)`/g)].map((m) => m[1].trim());
    for (const c of candidates) if (DOC_COMMAND.test(c)) found.add(c.replace(/^\$\s*/, '').slice(0, 140));
  }
  return [...found];
}

function readSmall(root, rel) {
  try {
    const full = join(root, rel);
    if (statSync(full).size > MAX_READ_BYTES) return '';
    return readFileSync(full, 'utf8');
  } catch {
    return '';
  }
}

/** The dependencies a manifest declares, as text the `deps` patterns can match; JSON manifests give only their keys. */
function declaredDependencies(file, text) {
  const name = basename(file);
  if (name !== 'package.json' && name !== 'composer.json' && !/^deno\.jsonc?$/.test(name)) return text;
  try {
    const json = JSON.parse(text);
    const groups = ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies', 'require', 'require-dev', 'imports'];
    return groups.flatMap((g) => Object.keys(json[g] ?? {})).map((d) => `"${d}"`).join(' ');
  } catch {
    return '';
  }
}

const lineCount = (text) => (text ? text.split('\n').length : 0);

function main() {
  if (process.argv[2] === '--help' || process.argv[2] === '-h') {
    const header = readFileSync(new URL(import.meta.url), 'utf8').split(/\r?\n/).slice(1);
    console.log(header.slice(0, header.findIndex((l) => !l.startsWith('//'))).map((l) => l.replace(/^\/\/ ?/, '')).join('\n'));
    return;
  }
  const root = resolve(process.argv[2] ?? '.');
  if (!existsSync(root) || !statSync(root).isDirectory()) {
    console.error(`inventory: ${root} is not a folder`);
    process.exit(2);
  }
  const { how, files, leftOut } = listFiles(root);
  let reads = 0;
  const read = (rel) => (reads++ < MAX_READ_FILES ? readSmall(root, rel) : '');

  const code = files.filter((f) => LANGUAGES[extname(f).toLowerCase()]);
  const isTest = (f) => TEST_PATH.test(f);
  const tests = new Set(code.filter(isTest));
  // Rust keeps unit tests inside the source file; such a file counts as tested in place.
  const inlineTested = new Set();
  for (const f of code.filter((x) => x.endsWith('.rs') && !tests.has(x))) if (/#\[cfg\(test\)\]|#\[test\]/.test(read(f))) inlineTested.add(f);

  const manifests = files.filter((f) => MANIFESTS.test(f));
  const ci = files.filter((f) => CI_FILES.test(f));
  const manifestText = manifests.map((f) => `\n${read(f)}`).join('');
  const configText = manifestText + ci.map((f) => `\n${read(f)}`).join('');
  const depText = manifests.filter((f) => DEPENDENCY_MANIFESTS.test(f)).map((f) => `\n${declaredDependencies(f, read(f))}\n`).join('');
  const testText = new Map([...tests].map((f) => [f, read(f)]));
  for (const f of inlineTested) testText.set(f, read(f));

  const out = [];
  out.push(`# Test inventory of ${root}`, '', `Files listed by ${how}: ${files.length}${leftOut ? ` (left out: ${leftOut})` : ''}. Read for signals: up to ${MAX_READ_FILES} files of at most ${MAX_READ_BYTES} bytes.`, '');

  // Languages
  const byLang = new Map();
  for (const f of code) {
    const lang = LANGUAGES[extname(f).toLowerCase()];
    const row = byLang.get(lang) ?? { source: 0, test: 0, inline: 0 };
    if (tests.has(f)) row.test++;
    else row.source++;
    if (inlineTested.has(f)) row.inline++;
    byLang.set(lang, row);
  }
  out.push('## Languages', '', '| Language | Source files | Test files | Source files with tests inside |', '|---|---|---|---|');
  for (const [lang, r] of [...byLang].sort((a, b) => b[1].source + b[1].test - (a[1].source + a[1].test))) out.push(`| ${lang} | ${r.source} | ${r.test} | ${r.inline || '-'} |`);
  if (byLang.size === 0) out.push('| (no source files recognised) | | | |');
  out.push('');

  // How tests run
  out.push('## How tests are run', '');
  out.push(manifests.length ? `Manifests and test configuration: ${manifests.join(', ')}` : 'No manifest or test configuration found.');
  for (const f of manifests.filter((x) => basename(x) === 'package.json')) {
    try {
      const scripts = JSON.parse(read(f)).scripts ?? {};
      const testish = Object.entries(scripts).filter(([k]) => /test|cover|e2e|spec|check|bench|mutat|fuzz|lint/i.test(k));
      out.push(testish.length ? `- ${f} scripts: ${testish.map(([k, v]) => `\`${k}\`: \`${v}\``).join('; ')}` : `- ${f}: no test script`);
    } catch {
      out.push(`- ${f}: not valid JSON`);
    }
  }
  if (ci.length) {
    out.push(`- CI and task files: ${ci.join(', ')}`);
    const commands = new Set();
    for (const f of ci) {
      for (const line of read(f).split('\n')) {
        if (/\b(test|tests|pytest|coverage|cargo (test|nextest|llvm-cov|mutants|fuzz)|go test|playwright|vitest|jest|mvn (test|verify)|gradle\w* test|dotnet test|stryker|mutmut|audit)\b/i.test(line)) commands.add(line.trim().slice(0, 140));
      }
    }
    for (const c of [...commands].slice(0, 15)) out.push(`  - \`${c}\``);
  } else out.push('- No CI or task file found: nothing runs the tests automatically.');
  for (const f of files.filter((x) => DOC_FILES.test(x))) {
    const commands = docCommands(read(f));
    if (commands.length) out.push(`- Test commands written in ${f}:`, ...commands.slice(0, 10).map((c) => `  - \`${c}\``));
  }
  out.push('');

  // Test files by folder
  out.push('## Test files', '');
  if (tests.size === 0 && inlineTested.size === 0) out.push('None found.');
  else {
    const byDir = new Map();
    for (const f of tests) {
      const dir = f.includes('/') ? f.slice(0, f.lastIndexOf('/')) : '.';
      byDir.set(dir, (byDir.get(dir) ?? 0) + 1);
    }
    out.push(`${tests.size} test file(s)${inlineTested.size ? `, and ${inlineTested.size} source file(s) with tests inside` : ''}. By folder (largest first):`);
    for (const [dir, n] of [...byDir].sort((a, b) => b[1] - a[1]).slice(0, 15)) out.push(`- ${dir}: ${n}`);
  }
  out.push('');

  // Kinds
  out.push('## Kinds of test', '', 'A signal is a lead, not proof: open the files named. Kinds with no signal may still exist under other names.', '', '| Kind | Signals found |', '|---|---|');
  const unitEvidence = tests.size || inlineTested.size ? `${tests.size + inlineTested.size} file(s) with tests` : '';
  out.push(`| example-based (unit, integration) | ${unitEvidence || 'none found'} |`);
  for (const k of KINDS) {
    const evidence = [];
    if (k.deps) {
      const names = new Set([...depText.matchAll(new RegExp(k.deps.source, `${k.deps.flags}g`))].map((m) => m[0].replace(/^["'\s]|["'\s=:<>~^/]$/g, '')));
      if (names.size) evidence.push(`${names.size > 1 ? 'dependencies' : 'dependency'} ${[...names].map((n) => `\`${n}\``).join(', ')}`);
    }
    if (k.files) {
      const hits = files.filter((f) => k.files.test(f) && (!k.testOnly || isTest(f) || /test|spec/i.test(basename(f)) || /(^|\/)scripts?\//.test(f)));
      if (hits.length) evidence.push(`${hits.length} path(s), e.g. ${hits.slice(0, 3).join(', ')}`);
    }
    if (k.code) {
      const hits = [...testText].filter(([, t]) => k.code.test(t)).map(([f]) => f);
      if (hits.length) evidence.push(`${hits.length} test file(s), e.g. ${hits.slice(0, 3).join(', ')}`);
    }
    if (k.config) {
      const m = configText.match(k.config);
      if (m) evidence.push(`configuration or CI: \`${m[0].trim().slice(0, 60)}\``);
    }
    out.push(`| ${k.kind} | ${evidence.join('; ') || 'none found'} |`);
  }
  out.push('');

  // Untested files
  const stems = [...tests].map((t) => basename(t).toLowerCase().replace(/\.[^.]+$/, '').replace(/^test_|[._-](test|spec|tests)$|_test$|(test|tests|spec|it)$/i, ''));
  const untested = code
    .filter((f) => !tests.has(f) && !inlineTested.has(f) && !/\.d\.ts$|(^|\/)(index|mod|main|lib|__init__)\.[^.]+$/.test(f))
    .filter((f) => {
      const stem = basename(f).toLowerCase().replace(/\.[^.]+$/, '');
      return !stems.some((s) => s === stem || s.startsWith(`${stem}.`) || s.startsWith(`${stem}_`) || s.startsWith(`${stem}-`));
    })
    .map((f) => ({ f, lines: lineCount(read(f)) }))
    .sort((a, b) => b.lines - a.lines)
    .slice(0, 15);
  out.push('## Largest source files with no test file of their name', '', 'A name match is a rough guess (a test may cover several files); a coverage run settles it.', '');
  if (untested.length === 0) out.push('None.');
  else for (const { f, lines } of untested) out.push(`- ${f} (${lines} lines)`);
  console.log(out.join('\n'));
}

main();
