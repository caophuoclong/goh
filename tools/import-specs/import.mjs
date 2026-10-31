import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { SourceGraph, convertSpec } from './static.mjs';
import { enhance } from './enhance.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const manifest = JSON.parse(await fs.readFile(path.join(here, 'sources.json'), 'utf8'));
const { repository, revision, commands, loadSpecs = [] } = manifest;
if (repository !== 'withfig/autocomplete' || !/^[a-f0-9]{40}$/.test(revision) || commands.some(c => !/^[a-z][a-z0-9_-]*$/.test(c))) throw new Error('Invalid pinned source manifest');
if (loadSpecs.some(name => !/^[a-z][a-z0-9-]*(?:\/[a-z][a-z0-9-]*)?$/.test(name))) throw new Error('Invalid selected loadSpec');
const check = process.argv.includes('--check');
const cache = path.join(root, 'target/spec-import', revision);
const fetched = new Map();
async function read(file) {
  if (file.startsWith('/') || file.split('/').includes('..')) throw new Error(`Unsafe source path: ${file}`);
  const local = path.join(cache, file);
  let text;
  try { text = await fs.readFile(local, 'utf8'); }
  catch (error) {
    if (error.code !== 'ENOENT') throw error;
    const url = `https://raw.githubusercontent.com/${repository}/${revision}/${file}`;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const response = await fetch(url, { signal: AbortSignal.timeout(30000) });
        if (!response.ok) {
          if (response.status < 500 && response.status !== 429) throw Object.assign(new Error(`${response.status}: ${url}`), { permanent: true });
          throw new Error(`${response.status}: ${url}`);
        }
        text = await response.text();
        break;
      } catch (error) {
        if (error.permanent || attempt === 2) throw new Error(`Cannot download ${file}`, { cause: error });
        await delay(250 * (attempt + 1));
      }
    }
    await fs.mkdir(path.dirname(local), { recursive: true });
    await fs.writeFile(local, text);
  }
  fetched.set(file, createHash('sha256').update(text).digest('hex'));
  return text;
}
const graph = new SourceGraph(read);
const outputs = new Map();
const reports = [];
for (const command of commands) {
  const source = `src/${command}.ts`;
  await graph.load(source);
  const omitted = [];
  const expanded = await graph.expand(graph.root(source), new Set(loadSpecs));
  const spec = convertSpec(expanded, command, omitted);
  const enhancements = enhance(spec);
  if (!spec.options?.length && !spec.subcommands?.length && !spec.args?.length) throw new Error(`Empty imported spec: ${command}`);
  outputs.set(`crates/goh-core/specs/${command}.json`, JSON.stringify(spec, null, 2) + '\n');
  reports.push({ command, source, subcommands: spec.subcommands?.length ?? 0, options: spec.options?.length ?? 0, omitted, enhancements });
}
const license = await read('LICENSE');
if (!license.startsWith('MIT License') || !license.includes('Hercules Labs Inc.')) throw new Error('Unexpected upstream license; review before importing');
outputs.set('third_party/fig-autocomplete/LICENSE', license);
const report = { repository, revision, license: 'MIT', files: Object.fromEntries([...fetched].sort()), commands: reports,
  parserOmissions: [...new Map(graph.issues.map(issue => [JSON.stringify(issue), issue])).values()].sort((a, b) => (a.source < b.source ? -1 : a.source > b.source ? 1 : a.line - b.line)) };
outputs.set('tools/import-specs/report.json', JSON.stringify(report, null, 2) + '\n');
// Finish conversion before changing any checked-in data.
for (const [file, text] of outputs) {
  const target = path.join(root, file);
  if (check) {
    if (await fs.readFile(target, 'utf8') !== text) throw new Error(`Generated file differs: ${file}`);
  } else {
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, text);
  }
}
console.log(`${check ? 'Verified' : 'Imported'} ${commands.length} commands from ${repository}@${revision.slice(0, 12)}; omissions: tools/import-specs/report.json`);
