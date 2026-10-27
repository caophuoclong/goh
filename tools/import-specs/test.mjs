import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SourceGraph, convertSpec } from './static.mjs';

async function imported(files, root = 'src/tool.ts', name = 'tool') {
  const graph = new SourceGraph(async file => {
    assert.ok(Object.hasOwn(files, file), `unexpected source read: ${file}`);
    return files[file];
  });
  await graph.load(root);
  const omissions = [];
  const spec = convertSpec(graph.root(root), name, omissions);
  return { spec, omissions, graph };
}

test('imports literal aliases, constant arrays, spreads and inherited options', async () => {
  const { spec } = await imported({
    'src/common.ts': 'export const flags = [{name:["-v","--verbose"], isPersistent:true}]; export default {args:{name:"path",template:["filepaths","folders"]}};',
    'src/tool.ts': 'import base, {flags} from "./common"; const child={name:["run","r"],...base}; const spec={name:"tool",options:[...flags],subcommands:[child]}; export default spec;',
  });
  assert.deepEqual(spec.subcommands.map(c => c.name), ['run', 'r']);
  for (const child of spec.subcommands) {
    assert.deepEqual(child.options.map(o => o.name), ['-v', '--verbose']);
    assert.equal(child.args[0].template, 'filepaths');
  }
});

test('does not execute top-level code, generators, dynamic spreads or snippets', async () => {
  const { spec, omissions, graph } = await imported({ 'src/tool.ts': `
    throw new Error('must never execute');
    const dynamic=(()=>{ throw new Error('must never call'); })();
    export default {name:'tool', generateSpec:dynamic, subcommands:[...dynamic,{name:'safe'}],
      options:[{name:'--value',args:{generators:{script:['sh','-c','touch /tmp/unsafe']},suggestions:[{name:'a;b',insertValue:'$(touch /tmp/unsafe)'}]}}]};
  ` });
  assert.deepEqual(spec.subcommands.map(c => c.name), ['safe']);
  assert.equal(spec.options[0].takesValue, true);
  assert.deepEqual(spec.options[0].args[0].suggestions, [{ name: 'a;b' }]);
  assert.ok(omissions.some(o => o.feature === 'generateSpec'));
  assert.ok(omissions.some(o => o.feature === 'generators'));
  assert.ok(omissions.some(o => o.feature === 'suggestion-insertValue'));
  assert.ok(graph.issues.some(o => o.feature === 'dynamic-array-spread'));
});

test('projects the known filepaths helper onto native folder enumeration without executing it', async () => {
  const { spec } = await imported({ 'src/tool.ts': 'import {filepaths} from "@fig/autocomplete-generators"; export default {name:"tool",args:{generators:filepaths({showFolders:"only"})}};' });
  assert.equal(spec.args[0].template, 'folders');
});

test('filters hidden and unsafe command names and keeps source aliases as static data', async () => {
  const { spec, omissions } = await imported({ 'src/tool.ts': `export default {name:'tool', subcommands:[{name:'ok'},{name:'bad;echo pwn'},{name:'secret',hidden:true}],options:[{name:['--good','--bad$(id)']}], args:{suggestions:[{name:['yes','no']},{name:'hidden',hidden:true}]}};` });
  assert.deepEqual(spec.subcommands.map(c => c.name), ['ok']);
  assert.deepEqual(spec.options.map(c => c.name), ['--good']);
  assert.deepEqual(spec.args[0].suggestions.map(c => c.name), ['yes', 'no']);
  assert.ok(omissions.some(o => o.feature === 'nonliteral-or-unsupported-name'));
});

test('handles cyclic imports conservatively and rejects paths outside src', async () => {
  const { spec } = await imported({
    'src/tool.ts': 'import {x} from "./cycle"; export const y=x; export default {name:"tool", options:[{name:"--safe"}],generateSpec:y};',
    'src/cycle.ts': 'import {y} from "./tool"; export const x=y;',
  });
  assert.equal(spec.options[0].name, '--safe');
  await assert.rejects(imported({ 'src/tool.ts': 'import data from "../../outside"; export default data;' }), /escapes source directory/);
});

test('extracts only object-bodied root factories and never runs defaults or statements', async () => {
  const { spec } = await imported({ 'src/tool.ts': 'const spec=(ignored=(()=>{throw Error("no");})())=>({name:"tool", options:[{name:"--safe"}]}); export default spec();' });
  assert.equal(spec.options[0].name, '--safe');
  await assert.rejects(imported({ 'src/tool.ts': 'const spec=()=>{throw Error("no"); return {name:"tool"};}; export default spec();' }), /No static command object/);
});

test('resolves explicitly selected static loadSpecs and reports the remaining references', async () => {
  const files = {
    'src/tool.ts': 'export default {name:"tool", subcommands:[{name:"storage",loadSpec:"tool/storage"},{name:"other",loadSpec:"tool/other"}]};',
    'src/tool/storage.ts': 'throw Error("never run"); export default {name:"storage",subcommands:[{name:"copy"}]};',
  };
  const graph = new SourceGraph(async file => { assert.ok(Object.hasOwn(files, file)); return files[file]; });
  await graph.load('src/tool.ts');
  const expanded = await graph.expand(graph.root('src/tool.ts'), new Set(['tool/storage']));
  const omitted = [];
  const spec = convertSpec(expanded, 'tool', omitted);
  assert.equal(spec.subcommands[0].subcommands[0].name, 'copy');
  assert.ok(omitted.some(item => item.at === 'tool/other' && item.feature === 'loadSpec'));
  assert.equal(omitted.filter(item => item.feature === 'loadSpec').length, 1);
});

test('projects only pinned Docker generator references and keeps nested command boundaries', async () => {
  const files = { 'src/docker.ts': `const dockerGenerators={runningDockerContainers:{script:['sh','-c','throw Error("never")']}};
    export default {name:'docker',subcommands:[{name:'exec',args:[{name:'container',generators:dockerGenerators.runningDockerContainers},{name:'command',isCommand:true}]}]};` };
  const {spec} = await imported(files, 'src/docker.ts', 'docker');
  assert.equal(spec.subcommands[0].args[0].provider, 'docker-containers');
  assert.equal(spec.subcommands[0].args[1].isCommand, true);
  assert.equal(spec.subcommands[0].args[1].isVariadic, true);
  assert.ok(!JSON.stringify(spec).includes('script'));
});
