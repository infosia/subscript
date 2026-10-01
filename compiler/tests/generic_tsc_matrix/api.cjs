// §143 rule 4: derive callable positions from the ambient syntax tree.
const fs = require('fs');
const ts = require(process.cwd() + '/node_modules/typescript');
const path = process.argv[2];
const source = ts.createSourceFile(path, fs.readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
const sites = [];
const omitted = [];
function text(node) { return node.getText(source).replace(/\s+/g, ' '); }
function visit(node, owner = '', ownerParameters = []) {
  if (ts.isModuleDeclaration(node)) {
    visit(node.body, node.name.text, []);
    return;
  }
  if (ts.isInterfaceDeclaration(node) || ts.isClassDeclaration(node)) {
    for (const member of node.members) visit(member, node.name.text, node.typeParameters || []);
    return;
  }
  if (ts.isFunctionDeclaration(node) || ts.isMethodSignature(node) || ts.isMethodDeclaration(node)) {
    const name = owner ? `${owner}.${text(node.name)}` : text(node.name);
    const parameters = [...(node.typeParameters || [])];
    let reason = '';

    function conditional(child) { return ts.isConditionalTypeNode(child) || ts.forEachChild(child, conditional); }
    if (conditional(node)) reason = 'The conditional signature needs a branch-specific call form.';
    if (name === 'Descriptor') reason = 'The constructor constraint needs a decorator target.';
    if (ts.isComputedPropertyName(node.name)) reason = 'The computed iterator name needs a protocol invocation.';
    if (reason) { omitted.push([name, reason]); return; }
    const generic = parameters.map(p => [p.name.text, p.constraint ? text(p.constraint) : '']);
    const outer = ownerParameters.map(p => [p.name.text, p.constraint ? text(p.constraint) : '']);
    const args = node.parameters.map(p => text(p.type));
    const staticMember = node.modifiers?.some(m => m.kind === ts.SyntaxKind.StaticKeyword);
    sites.push({name, owner, generic, outer, args, staticMember});
    return;
  }
  const memberName = node.name ? text(node.name) : ts.SyntaxKind[node.kind];
  const qualified = owner ? `${owner}.${memberName}` : memberName;
  const unsupported = [
    [ts.isConstructorDeclaration, 'constructor', 'The constructor needs a new-expression call form.'],
    [ts.isConstructSignatureDeclaration, 'construct', 'The construct signature needs a new-expression call form.'],
    [ts.isCallSignatureDeclaration, 'call', 'The call signature needs an object invocation.'],
    [ts.isPropertySignature, memberName, 'The property needs a member-value form.'],
    [ts.isPropertyDeclaration, memberName, 'The property needs a member-value form.'],
    [ts.isGetAccessorDeclaration, memberName, 'The getter needs a member-read form.'],
    [ts.isSetAccessorDeclaration, memberName, 'The setter needs a member-write form.'],
    [ts.isIndexSignatureDeclaration, 'index', 'The index signature needs an indexed-access form.'],
  ];
  for (const [test, label, reason] of unsupported) {
    if (test(node)) { omitted.push([owner ? `${owner}.${label}` : label, reason]); return; }
  }
  if (ts.isVariableStatement(node)) {
    for (const declaration of node.declarationList.declarations) {
      omitted.push([owner ? `${owner}.${text(declaration.name)}` : text(declaration.name),
        'The variable needs a value or function-value invocation.']);
    }
    return;
  }
  if (owner && !ts.isModuleBlock(node)) {
    throw new Error(`Unknown ambient member ${qualified}: ${ts.SyntaxKind[node.kind]}`);
  }
  ts.forEachChild(node, child => visit(child, owner, ownerParameters));
}
visit(source);
const counts = new Map();
for (const site of sites) counts.set(site.name, (counts.get(site.name) || 0) + 1);
for (const [name, count] of counts) {
  if (count > 1) omitted.push([name, 'The overload set needs one call form for each signature.']);
}
for (const [name, reason] of omitted) console.log(`omit\t${name}\t${reason}`);
for (const site of sites) {
  if (counts.get(site.name) > 1) continue;
  console.log(['call', site.name, site.owner, site.staticMember ? 'static' : '',
    site.outer.map(p => p.join('~')).join(';'), site.generic.map(p => p.join('~')).join(';'),
    site.args.join('\t')].join('\t'));
}
