import { spawnSync } from 'node:child_process';

const ALLOWED = new Map([
  ['ats-kernel', []],
  ['ats-runtime', ['ats-kernel']],
  ['ats-game-context', ['ats-kernel']],
  ['ats-workspace', ['ats-kernel']],
  ['ats-features', ['ats-game-context', 'ats-kernel', 'ats-runtime', 'ats-workspace']],
  ['ats-adapters', ['ats-game-context', 'ats-kernel', 'ats-runtime', 'ats-workspace']],
]);

export function validateStage2Dag(packages) {
  const byName = new Map(packages.map((pkg) => [pkg.name, pkg]));
  const errors = [];

  for (const [name, allowed] of ALLOWED) {
    const pkg = byName.get(name);
    if (!pkg) {
      errors.push(`missing target crate: ${name}`);
      continue;
    }

    const actual = pkg.dependencies
      .filter((dependency) => dependency.source === null)
      .map((dependency) => dependency.name)
      .sort();
    const expected = [...allowed].sort();

    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      errors.push(`${name}: expected [${expected.join(', ')}], found [${actual.join(', ')}]`);
    }
  }

  return errors;
}

function dependency(name) {
  return { name, source: null };
}

function validFixture() {
  return [...ALLOWED].map(([name, dependencies]) => ({
    name,
    dependencies: dependencies.map(dependency),
  }));
}

function runSelfTest() {
  const valid = validFixture();
  if (validateStage2Dag(valid).length !== 0) {
    throw new Error('valid dependency fixture was rejected');
  }

  const reverse = structuredClone(valid);
  reverse.find((pkg) => pkg.name === 'ats-runtime').dependencies.push(dependency('ats-features'));
  if (!validateStage2Dag(reverse).some((error) => error.startsWith('ats-runtime:'))) {
    throw new Error('runtime-to-feature dependency was not rejected');
  }

  const missing = structuredClone(valid);
  missing.find((pkg) => pkg.name === 'ats-features').dependencies = [];
  if (!validateStage2Dag(missing).some((error) => error.startsWith('ats-features:'))) {
    throw new Error('missing feature dependencies were not rejected');
  }

  const legacy = structuredClone(valid);
  legacy.find((pkg) => pkg.name === 'ats-workspace').dependencies.push(dependency('ats-core'));
  if (!validateStage2Dag(legacy).some((error) => error.startsWith('ats-workspace:'))) {
    throw new Error('new-crate dependency on ats-core was not rejected');
  }

  const shell = structuredClone(valid);
  shell.find((pkg) => pkg.name === 'ats-adapters').dependencies.push(dependency('ats-web'));
  if (!validateStage2Dag(shell).some((error) => error.startsWith('ats-adapters:'))) {
    throw new Error('new-crate dependency on a shell was not rejected');
  }

  process.stdout.write('stage2 dependency DAG self-test passed\n');
}

function readCargoMetadata() {
  const command = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  const result = spawnSync(command, ['metadata', '--no-deps', '--format-version', '1'], {
    cwd: new URL('..', import.meta.url),
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    process.stderr.write(result.stderr || 'cargo metadata failed\n');
    process.exit(result.status ?? 1);
  }
  return JSON.parse(result.stdout);
}

if (process.argv.includes('--self-test')) {
  runSelfTest();
} else {
  const metadata = readCargoMetadata();
  const errors = validateStage2Dag(metadata.packages);
  if (errors.length > 0) {
    process.stderr.write(`Stage 2 dependency DAG violation:\n${errors.map((error) => `- ${error}`).join('\n')}\n`);
    process.exit(1);
  }
  process.stdout.write('stage2 dependency DAG check passed\n');
}
