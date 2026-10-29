import ts from 'typescript';
import path from 'node:path';

class Unknown {
  constructor(reason) { this.reason = reason; }
}
const unknown = reason => new Unknown(reason);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value) && !(value instanceof Unknown);
const list = value => value == null ? [] : Array.isArray(value) ? value : [value];
const clean = (value, limit) => typeof value === 'string' ? Array.from(value.replace(/[\x00-\x1f\x7f]/g, ' ')).slice(0, limit).join('') : undefined;

// Read declarations as data. Never transpile, import, eval, or call upstream JavaScript.
export class SourceGraph {
  constructor(read) { this.read = read; this.modules = new Map(); this.cache = new Map(); this.issues = []; }
  async load(file) {
    if (this.modules.has(file)) return;
    const text = await this.read(file);
    const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
    if (ast.parseDiagnostics.length) throw new Error(`Cannot parse ${file}: ${ast.parseDiagnostics[0].messageText}`);
    const module = { file, ast, bindings: new Map(), imports: new Map(), exports: new Map() };
    this.modules.set(file, module); // Register before following imports, including cycles.
    for (const node of ast.statements) {
      if (ts.isVariableStatement(node)) {
        for (const declaration of node.declarationList.declarations) {
          if (ts.isIdentifier(declaration.name) && declaration.initializer) module.bindings.set(declaration.name.text, declaration.initializer);
        }
      } else if (ts.isExportAssignment(node)) module.exports.set('default', node.expression);
      else if (ts.isExportDeclaration(node) && node.exportClause && ts.isNamedExports(node.exportClause) && !node.moduleSpecifier) {
        for (const item of node.exportClause.elements) module.exports.set(item.name.text, item.propertyName ?? item.name);
      } else if (ts.isImportDeclaration(node) && node.importClause) {
        const source = node.moduleSpecifier.text;
        let target;
        if (source.startsWith('.')) {
          target = path.posix.normalize(path.posix.join(path.posix.dirname(file), source));
          if (!target.endsWith('.ts')) target += '.ts';
          if (!target.startsWith('src/')) throw new Error(`Import escapes source directory: ${source}`);
        }
        const bind = (local, exported) => module.imports.set(local, { target, source, exported });
        if (node.importClause.name) bind(node.importClause.name.text, 'default');
        const bindings = node.importClause.namedBindings;
        if (bindings && ts.isNamedImports(bindings)) for (const item of bindings.elements) bind(item.name.text, (item.propertyName ?? item.name).text);
      }
    }
    for (const { target } of module.imports.values()) if (target) await this.load(target);
  }
  value(file, name = 'default', stack = new Set()) {
    const key = `${file}:${name}`;
    if (stack.has(key)) return unknown(`cyclic reference ${key}`);
    if (this.cache.has(key)) return this.cache.get(key);
    const module = this.modules.get(file);
    if (!module) return unknown(`unloaded module ${file}`);
    const next = new Set(stack).add(key);
    const expression = module.exports.get(name) ?? module.bindings.get(name);
    const imported = module.imports.get(name);
    const value = expression ? this.evaluate(expression, module, next)
      : imported?.target ? this.value(imported.target, imported.exported, next) : unknown(`unresolved reference ${key}`);
    this.cache.set(key, value);
    return value;
  }
  root(file) {
    const module = this.modules.get(file);
    const exported = module.exports.get('default');
    // Some specs export a named factory whose entire body is an object literal (cargo).
    // Inspect that literal; never run statements, parameter defaults or the factory itself.
    if (exported && ts.isCallExpression(exported) && ts.isIdentifier(exported.expression) && exported.arguments.length === 0) {
      const factory = module.bindings.get(exported.expression.text);
      if (factory && ts.isArrowFunction(factory)) {
        let body = factory.body;
        while (ts.isParenthesizedExpression(body)) body = body.expression;
        if (ts.isObjectLiteralExpression(body)) return this.evaluate(body, module, new Set());
      }
    }
    return this.value(file);
  }
  async expand(source, allowed, stack = new Set()) {
    if (!object(source)) return source;
    let result = { ...source };
    if (typeof result.loadSpec === 'string' && allowed.has(result.loadSpec)) {
      const file = `src/${result.loadSpec}.ts`;
      if (stack.has(file)) throw new Error(`Cyclic loadSpec: ${file}`);
      await this.load(file);
      const loaded = this.root(file);
      if (!object(loaded)) throw new Error(`No static delegated spec: ${file}`);
      result = { ...loaded, ...result };
      delete result.loadSpec;
      stack = new Set(stack).add(file);
    }
    if (Array.isArray(result.subcommands)) result.subcommands = await Promise.all(result.subcommands.map(child => this.expand(child, allowed, stack)));
    return result;
  }
  evaluate(node, module, stack) {
    const read = item => this.evaluate(item, module, stack);
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return node.text;
    if (ts.isNumericLiteral(node)) return Number(node.text);
    if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
    if (node.kind === ts.SyntaxKind.NullKeyword) return null;
    if (ts.isIdentifier(node)) return node.text === 'undefined' ? undefined : this.value(module.file, node.text, stack);
    if (ts.isAsExpression(node) || ts.isSatisfiesExpression(node) || ts.isTypeAssertionExpression(node) || ts.isParenthesizedExpression(node)) return read(node.expression);
    if (ts.isArrayLiteralExpression(node)) {
      const values = [];
      for (const element of node.elements) {
        if (ts.isSpreadElement(element)) {
          const spread = read(element.expression);
          if (Array.isArray(spread)) values.push(...spread);
          else { values.push(unknown('dynamic array spread')); this.note(module, element, 'dynamic-array-spread'); }
        } else values.push(read(element));
      }
      return values;
    }
    if (ts.isObjectLiteralExpression(node)) {
      const value = Object.create(null);
      for (const property of node.properties) {
        if (ts.isSpreadAssignment(property)) {
          const spread = read(property.expression);
          if (object(spread)) Object.assign(value, spread);
          else this.note(module, property, 'dynamic-object-spread');
        } else if (ts.isPropertyAssignment(property) || ts.isShorthandPropertyAssignment(property)) {
          const name = property.name;
          const key = ts.isComputedPropertyName(name) ? read(name.expression) : name.text;
          if (typeof key === 'string' && !['__proto__', 'constructor', 'prototype'].includes(key)) value[key] = read(ts.isPropertyAssignment(property) ? property.initializer : name);
        } else if (property.name && !ts.isComputedPropertyName(property.name)) {
          value[property.name.text] = unknown(ts.SyntaxKind[property.kind]);
        }
      }
      return value;
    }
    if (ts.isPropertyAccessExpression(node)) {
      if (module.file === 'src/docker.ts' && ts.isIdentifier(node.expression) && node.expression.text === 'dockerGenerators') {
        const providers = { runningDockerContainers: 'docker-containers', allDockerContainers: 'docker-all-containers', allLocalImages: 'docker-images', allLocalImagesWithRepository: 'docker-images' };
        if (Object.hasOwn(providers, node.name.text)) return { gohProvider: providers[node.name.text] };
      }
      const value = read(node.expression);
      return object(value) && Object.hasOwn(value, node.name.text) ? value[node.name.text] : unknown('dynamic property access');
    }
    if (ts.isTemplateExpression(node)) {
      let text = node.head.text;
      for (const span of node.templateSpans) {
        const value = read(span.expression);
        if (!['string', 'number', 'boolean'].includes(typeof value)) return unknown('dynamic template');
        text += String(value) + span.literal.text;
      }
      return text;
    }
    if (ts.isBinaryExpression(node)) {
      const left = read(node.left);
      if (left instanceof Unknown) return left;
      if (node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) return left ? read(node.right) : left;
      if (node.operatorToken.kind === ts.SyntaxKind.BarBarToken) return left ? left : read(node.right);
      const right = read(node.right);
      if (node.operatorToken.kind === ts.SyntaxKind.InKeyword && typeof left === 'string' && object(right)) return Object.hasOwn(right, left);
      if (node.operatorToken.kind === ts.SyntaxKind.PlusToken && typeof left === 'string' && typeof right === 'string') return left + right;
    }
    // This one known helper maps to goh's own filesystem template; no helper code is run.
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      const imported = module.imports.get(node.expression.text);
      if (imported?.source === '@fig/autocomplete-generators' && imported.exported === 'filepaths') {
        const settings = node.arguments.length ? read(node.arguments[0]) : {};
        if (settings?.extensions) this.note(module, node, 'file-extension-filter-not-supported');
        return { gohTemplate: settings?.showFolders === 'only' ? 'folders' : 'filepaths' };
      }
    }
    return unknown(ts.SyntaxKind[node.kind]);
  }
  note(module, node, feature) {
    const line = module.ast.getLineAndCharacterOfPosition(node.getStart(module.ast)).line + 1;
    this.issues.push({ source: module.file, line, feature });
  }
}

export function convertSpec(raw, command, report) {
  const issue = (at, feature) => report.push({ at, feature });
  const names = (value, at, option = false) => list(value).flatMap(name => {
    if (typeof name !== 'string' || name.length > 256 || !(option ? /^(?:--?[a-zA-Z0-9][a-zA-Z0-9._-]*|-[@#:]|--)$/.test(name) : /^[a-zA-Z0-9][a-zA-Z0-9_.:+-]*$/.test(name))) {
      issue(at, 'nonliteral-or-unsupported-name'); return [];
    }
    return [name];
  });
  const metadata = (source, target) => {
    const description = clean(source.description, 1024);
    if (description) target.description = description;
  };
  const unsupported = (source, at) => {
    for (const key of ['generators', 'generateSpec', 'loadSpec', 'parserDirectives', 'exclusiveOn', 'dependsOn', 'requiresSeparator', 'insertValue']) {
      if (Object.hasOwn(source, key)) issue(at, key);
    }
  };
  const args = (source, at) => list(source).map((arg, index) => {
    const where = `${at}/args/${index}`;
    if (!object(arg)) { issue(where, 'dynamic-argument'); return { name: 'argument' }; }
    unsupported(arg, where);
    const out = { name: clean(arg.name, 256) || 'argument' };
    metadata(arg, out);
    if (arg.isVariadic === true || arg.isCommand === true) out.isVariadic = true;
    if (arg.isCommand === true) out.isCommand = true;
    if (arg.generators?.gohProvider) out.provider = arg.generators.gohProvider;
    if (arg.isOptional === true) out.isOptional = true;
    const templates = [...list(arg.template), ...list(arg.generators?.template)];
    if (templates.includes('filepaths')) out.template = 'filepaths';
    else if (templates.includes('folders')) out.template = 'folders';
    else if (arg.generators?.gohTemplate) out.template = arg.generators.gohTemplate;
    else if (arg.isScript === true) out.template = 'filepaths';
    const suggestions = [];
    for (const value of list(arg.suggestions)) {
      if (typeof value === 'string') suggestions.push({ name: clean(value, 256) });
      else if (object(value) && value.hidden !== true) for (const name of list(value.name)) {
        if (typeof name === 'string' && name) {
          const suggestion = { name: clean(name, 256) };
          metadata(value, suggestion);
          if (value.insertValue !== undefined) issue(where, 'suggestion-insertValue');
          suggestions.push(suggestion); // goh escapes names itself; never import executable snippets.
        }
      } else if (value instanceof Unknown) issue(where, 'dynamic-suggestions');
    }
    if (suggestions.length) out.suggestions = suggestions.filter(s => s.name).slice(0, 1000);
    return out;
  }).slice(0, 16);
  const options = (source, at) => list(source).flatMap((option, index) => {
    if (!object(option)) { issue(`${at}/options/${index}`, 'dynamic-option'); return []; }
    if (option.hidden === true) return [];
    const where = `${at}/options/${index}`;
    unsupported(option, where);
    return names(option.name, where, true).map(name => {
      const out = { name };
      metadata(option, out);
      if (option.args !== undefined && option.args !== null) { out.takesValue = true; out.args = args(option.args, where); }
      else out.takesValue = false;
      if (option.isRepeatable === true || (typeof option.isRepeatable === 'number' && option.isRepeatable > 1)) out.isRepeatable = true;
      return { spec: out, persistent: option.isPersistent === true };
    });
  });
  const node = (source, name, at, inherited = []) => {
    if (!object(source)) throw new Error(`No static command object for ${at}`);
    unsupported(source, at);
    const out = { name };
    metadata(source, out);
    const own = options(source.options, at);
    const merged = [...inherited, ...own.map(o => o.spec)];
    out.options = [...new Map(merged.map(o => [o.name, o])).values()].slice(0, 1000);
    if (!out.options.length) delete out.options;
    if (source.args !== undefined) out.args = args(source.args, at);
    if (command === 'docker') for (const arg of out.args ?? []) {
      if (arg.name === 'image' && !arg.provider) arg.provider = 'docker-images';
      if (arg.name === 'command') { arg.isVariadic = true; arg.isCommand = true; }
    }
    if (out.args?.some((a, i) => a.isVariadic && i < out.args.length - 1)) issue(at, 'variadic-before-last-argument');
    const persistent = [...inherited, ...own.filter(o => o.persistent).map(o => o.spec)];
    const children = [];
    for (const child of list(source.subcommands)) {
      if (!object(child)) { issue(at, 'dynamic-subcommands'); continue; }
      if (child.hidden === true) continue;
      for (const childName of names(child.name, at)) children.push(node(child, childName, `${at}/${childName}`, persistent));
    }
    if (children.length) out.subcommands = [...new Map(children.map(c => [c.name, c])).values()].slice(0, 1000);
    return out;
  };
  return { schemaVersion: 1, ...node(raw, command, command) };
}
