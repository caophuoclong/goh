// Small, reviewed additions where the pinned declarations omit standard CLI argument slots/flags.
// These are data only. No installed CLI is run by the importer.
export function enhance(spec) {
  const changes = [];
  const addOptions = (node, options) => {
    node.options ??= [];
    for (const option of options) if (!node.options.some(o => o.name === option.name)) node.options.push(option);
  };
  const value = (name, description, arg) => ({name, description, takesValue:true, args:[{name:arg}]});
  if (spec.name === 'docker') {
    addOptions(spec, [
      value('--context', 'Docker context to use', 'context'), value('-c', 'Docker context to use', 'context'),
      value('--host', 'Docker daemon endpoint', 'host'), value('-H', 'Docker daemon endpoint', 'host'),
      value('--config', 'Client configuration directory', 'directory'),
    ]);
    changes.push({source:'https://docs.docker.com/reference/cli/docker/', feature:'global-context-selectors'});
  }
  if (spec.name === 'go') {
    for (const name of ['build', 'test', 'list', 'vet', 'fmt', 'clean', 'install', 'get']) {
      const node = spec.subcommands?.find(s => s.name === name);
      if (node && !node.args?.length) node.args = [{name:'package', isVariadic:true, template:'folders'}];
    }
    const test = spec.subcommands?.find(s => s.name === 'test');
    if (test) addOptions(test, [
      value('-run', 'Select tests by regular expression', 'regexp'),
      value('-count', 'Repeat each selected test', 'count'),
      value('-timeout', 'Limit test execution duration', 'duration'),
      value('-bench', 'Select benchmarks by regular expression', 'regexp'),
      {name:'-short', description:'Run shorter tests', takesValue:false},
      {name:'-cover', description:'Report test coverage', takesValue:false},
      {...value('-coverprofile', 'Write a coverage profile', 'file'), args:[{name:'file',template:'filepaths'}]},
    ]);
    changes.push({source:'https://pkg.go.dev/cmd/go', feature:'package-arguments-and-common-test-flags'});
  }
  const walk = (node, visit) => { visit(node); for (const child of node.subcommands ?? []) walk(child, visit); };
  const withProvider = (option, provider) => { option.args = [{...(option.args?.[0] ?? {name: option.name}), provider}]; option.takesValue = true; };
  if (spec.name === 'cargo') {
    // Targets and features come from the manifest in the working directory (read as text, nothing is run).
    const providers = {'--bin': 'cargo-bins', '--example': 'cargo-examples', '-p': 'workspace-packages', '--package': 'workspace-packages', '--features': 'cargo-features', '-F': 'cargo-features'};
    walk(spec, node => { for (const option of node.options ?? []) if (providers[option.name]) withProvider(option, providers[option.name]); });
    changes.push({source:'https://doc.rust-lang.org/cargo/reference/cargo-targets.html', feature:'manifest-targets-features-packages'});
  }
  if (spec.name === 'pnpm') {
    walk(spec, node => { for (const option of node.options ?? []) if (option.name === '--filter' || option.name === '-F') option.args = [{...option.args?.[0], provider: 'workspace-packages'}]; });
    changes.push({source:'https://pnpm.io/filtering', feature:'workspace-package-filter'});
  }
  if (spec.name === 'docker') {
    const compose = spec.subcommands?.find(s => s.name === 'compose');
    walk(compose ?? {}, node => { for (const arg of node.args ?? []) if (arg.name === 'service') arg.provider = 'compose-services'; });
    // `-f` is the compose file only before the subcommand (`logs -f` follows, `rm -f` forces).
    for (const option of compose?.options ?? []) if (option.name === '-f' || option.name === '--file') option.args = [{name: 'file', template: 'filepaths', extensions: ['yml', 'yaml']}];
    changes.push({source:'https://docs.docker.com/reference/cli/docker/compose/', feature:'compose-file-services'});
  }
  const only = (arg, extensions) => { if (arg) arg.extensions = extensions; };
  if (spec.name === 'python' || spec.name === 'python3') only(spec.args?.find(a => a.template === 'filepaths'), ['py', 'pyw']);
  if (spec.name === 'node') only(spec.args?.find(a => a.template === 'filepaths'), ['js', 'mjs', 'cjs', 'ts', 'mts', 'cts']);
  if (spec.name === 'unzip') only(spec.args?.find(a => a.template === 'filepaths'), ['zip', 'jar', 'whl', 'apk']);
  if (spec.name === 'tar') {
    // Reading an archive means it exists already; creating one (`c`) names a new file.
    for (const mode of ['x', 't']) for (const option of spec.subcommands?.find(s => s.name === mode)?.options ?? [])
      if (option.name === '-f' || option.name === '--file') option.args = [{name: 'ARCHIVE', template: 'filepaths', extensions: ['tar', 'tgz', 'tar.gz', 'tbz2', 'tar.bz2', 'txz', 'tar.xz', 'tzst', 'tar.zst']}];
  }
  if (['python', 'python3', 'node', 'unzip', 'tar'].includes(spec.name)) changes.push({source:'https://pubs.opengroup.org/onlinepubs/9699919799/utilities/', feature:'file-extension-filter'});
  return changes;
}
