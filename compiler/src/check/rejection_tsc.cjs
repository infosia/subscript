// One stock TypeScript program returns syntax and semantic diagnostics.
const ts = require(process.argv[2]);
const path = require('path');
const configPath = process.argv[3];
const config = ts.readConfigFile(configPath, ts.sys.readFile);
if (config.error) throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText, '\n'));
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configPath));
if (parsed.errors.length) throw new Error(ts.formatDiagnostics(parsed.errors, {getCanonicalFileName:x=>x,getCurrentDirectory:()=>process.cwd(),getNewLine:()=> '\n'}));
const program = ts.createProgram(parsed.fileNames, parsed.options);
for (const diagnostic of ts.getPreEmitDiagnostics(program)) {
  if (!diagnostic.file) throw new Error(ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'));
  const relative = path.relative(path.dirname(configPath), diagnostic.file.fileName);
  const parts = relative.split(path.sep);
  const owner = parts[0] === 'general' ? '@' + parts[1] : path.basename(relative);
  process.stdout.write(owner + '\tTS' + diagnostic.code + '\n');
}
