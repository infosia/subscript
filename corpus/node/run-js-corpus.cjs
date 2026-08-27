"use strict";

const fs = require("node:fs");
const Module = require("node:module");
const pathModule = require("node:path");
const ts = require("typescript");

const shimNames = require("./js-corpus-shim.cjs");
const write = process.stdout.write.bind(process.stdout);

function bytes(value, encoding) {
  if (Buffer.isBuffer(value)) {
    return value;
  }
  return Buffer.from(value, typeof encoding === "string" ? encoding : "utf8");
}

async function runEntry(path) {
  const chunks = [];
  process.stdout.write = (value, encoding, callback) => {
    chunks.push(bytes(value, encoding));
    if (typeof encoding === "function") {
      encoding();
    } else if (typeof callback === "function") {
      callback();
    }
    return true;
  };

  try {
    const source = fs.readFileSync(path, "utf8");
    const javascript = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
      },
      fileName: path,
    }).outputText;
    const loaded = new Module(path, module);
    loaded.filename = path;
    loaded.paths = Module._nodeModulePaths(pathModule.dirname(path));
    loaded._compile(javascript, path);
    const entry = loaded.exports;
    if (typeof entry.main !== "function") {
      throw new Error("the entry does not export main");
    }
    await entry.main();
    return ["ok", Buffer.concat(chunks), Buffer.alloc(0)];
  } catch (error) {
    const detail = error && error.stack ? error.stack : String(error);
    return ["error", Buffer.concat(chunks), Buffer.from(detail, "utf8")];
  } finally {
    process.stdout.write = write;
  }
}

(async () => {
  write(`meta\t${process.version}\t${ts.version}\t${shimNames.join(",")}\n`);
  for (let index = 0; index < process.argv.length - 2; index += 1) {
    const [status, output, error] = await runEntry(process.argv[index + 2]);
    write(`${index}\t${status}\t${output.toString("hex")}\t${error.toString("hex")}\n`);
  }
})().catch((error) => {
  process.stderr.write(`${error && error.stack ? error.stack : error}\n`);
  process.exitCode = 1;
});
