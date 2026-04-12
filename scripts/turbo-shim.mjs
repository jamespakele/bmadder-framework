#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const repoRoot = process.cwd();
const [, , command, taskName, ...rest] = process.argv;
const isDryRun = rest.includes('--dry');

if (command !== 'run' || !taskName) {
  console.error('Usage: turbo run <build|test|lint> [--dry]');
  process.exit(1);
}

const rootPackage = JSON.parse(readFileSync(path.join(repoRoot, 'package.json'), 'utf8'));
const workspacePatterns = rootPackage.workspaces ?? [];
const workspacePackages = workspacePatterns.flatMap(resolveWorkspacePattern);

if (workspacePackages.length === 0) {
  console.error('No workspaces found.');
  process.exit(1);
}

for (const workspace of workspacePackages) {
  const packageJsonPath = path.join(repoRoot, workspace, 'package.json');
  if (!existsSync(packageJsonPath)) {
    continue;
  }

  const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
  const script = packageJson.scripts?.[taskName];

  if (!script) {
    continue;
  }

  if (isDryRun) {
    console.log(`${packageJson.name}: ${script}`);
    continue;
  }

  console.log(`Running ${taskName} in ${packageJson.name}`);
  const result = spawnSync('npm', ['run', taskName], {
    cwd: path.join(repoRoot, workspace),
    stdio: 'inherit',
    shell: false,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function resolveWorkspacePattern(pattern) {
  if (!pattern.endsWith('/*')) {
    return existsSync(path.join(repoRoot, pattern, 'package.json')) ? [pattern] : [];
  }

  const baseDirectory = pattern.slice(0, -2);
  const absoluteBaseDirectory = path.join(repoRoot, baseDirectory);

  if (!existsSync(absoluteBaseDirectory)) {
    return [];
  }

  return readdirSync(absoluteBaseDirectory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.posix.join(baseDirectory, entry.name))
    .filter((workspacePath) => existsSync(path.join(repoRoot, workspacePath, 'package.json')));
}
