import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function readobj() {
  const candidates = [];
  if (process.env.LLVM_PREFIX) {
    candidates.push(join(process.env.LLVM_PREFIX, "bin", "llvm-readobj.exe"));
    candidates.push(join(process.env.LLVM_PREFIX, "bin", "llvm-readobj"));
  }
  if (process.env.TSUZURI_CLANG && /[\\/]/.test(process.env.TSUZURI_CLANG)) {
    const dir = dirname(process.env.TSUZURI_CLANG);
    candidates.push(join(dir, "llvm-readobj.exe"));
    candidates.push(join(dir, "llvm-readobj"));
  }
  if (process.platform === "win32") {
    candidates.push("C:\\Program Files\\LLVM\\bin\\llvm-readobj.exe");
  }
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }
  for (const name of ["llvm-readobj.exe", "llvm-readobj"]) {
    const probe = spawnSync(name, ["--version"], { encoding: "utf8" });
    if (!probe.error && probe.status === 0) return name;
  }
  throw new Error("llvm-readobj was not found in LLVM_PREFIX/bin, beside TSUZURI_CLANG, or on PATH");
}

function runReadobj(args) {
  const tool = readobj();
  const result = spawnSync(tool, args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${tool} ${args.join(" ")}\n${result.stderr}`);
  return result.stdout;
}

export function peHeaders(file) {
  const text = runReadobj(["--file-headers", file]);
  const machineMatch = text.match(/^\s*Machine:\s+\S+\s+\(0x([0-9a-fA-F]+)\)/m);
  assert.ok(machineMatch, `missing Machine in ${file}:\n${text}`);
  const machineCode = Number.parseInt(machineMatch[1], 16);
  const machine = machineCode === 0x8664 ? "x64" : machineCode === 0xaa64 ? "arm64" : `0x${machineCode.toString(16)}`;
  const subsystemMatch = text.match(/^\s*Subsystem:\s+\S+\s+\(0x([0-9a-fA-F]+)\)/m);
  const subsystem = subsystemMatch ? Number.parseInt(subsystemMatch[1], 16) : null;
  const dllCharacteristics = [];
  const optStart = text.indexOf("ImageOptionalHeader {");
  if (optStart >= 0) {
    const optBlock = text.slice(optStart);
    const charMatch = optBlock.match(/Characteristics\s*\[\s*\(0x[0-9a-fA-F]+\)([\s\S]*?)\n\s*\]/);
    if (charMatch) {
      for (const line of charMatch[1].split(/\r?\n/)) {
        const flag = line.trim().match(/^(IMAGE_DLL_CHARACTERISTICS_\S+)/);
        if (flag) dllCharacteristics.push(flag[1]);
      }
    }
  }
  return { machine, subsystem, dllCharacteristics };
}

export function peSections(file) {
  const text = runReadobj(["--sections", file]);
  const sections = [];
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(/^\s*Name:\s+(\S+)/);
    if (match) sections.push(match[1]);
  }
  return sections;
}

function parseResIdentifier(raw) {
  const idMatch = raw.match(/\(ID\s+(\d+)\)/);
  if (idMatch) return Number.parseInt(idMatch[1], 10);
  const nameMatch = raw.match(/\(Name:\s+([^)]+)\)/);
  if (nameMatch) return nameMatch[1];
  const bare = raw.trim().replace(/\s*\[$/, "");
  return /^\d+$/.test(bare) ? Number.parseInt(bare, 10) : bare;
}

export function peResources(file) {
  const text = runReadobj(["--coff-resources", file]);
  const resources = [];
  let currentType = null;
  let currentName = null;
  for (const line of text.split(/\r?\n/)) {
    const typeMatch = line.match(/^\s*Type:\s+(.+?)\s*\[$/);
    if (typeMatch) {
      currentType = parseResIdentifier(typeMatch[1]);
      currentName = null;
      continue;
    }
    const nameMatch = line.match(/^\s*Name:\s+(.+?)\s*\[$/);
    if (nameMatch) {
      currentName = parseResIdentifier(nameMatch[1]);
      continue;
    }
    const langMatch = line.match(/^\s*Language:\s+(.+?)\s*\[$/);
    if (langMatch && currentType !== null && currentName !== null) {
      resources.push({
        type: currentType,
        name: currentName,
        language: parseResIdentifier(langMatch[1]),
      });
    }
  }
  return resources;
}

export function unwindFunctions(object) {
  const text = runReadobj(["--unwind", object]);
  const names = new Set();
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(/^\s*(?:StartAddress|Function):\s+([^\s(+]+)/);
    if (match) names.add(match[1]);
  }
  return names;
}

export function writeTestRes(file, entries) {
  const chunks = [];
  const empty = Buffer.alloc(32);
  empty.writeUInt32LE(0, 0);
  empty.writeUInt32LE(32, 4);
  empty.writeUInt16LE(0xffff, 8);
  empty.writeUInt16LE(0, 10);
  empty.writeUInt16LE(0xffff, 12);
  empty.writeUInt16LE(0, 14);
  chunks.push(empty);
  for (const { type, name, language = 1033, data = "" } of entries) {
    const payload = Buffer.isBuffer(data) ? data : Buffer.from(data, "utf8");
    const header = Buffer.alloc(32);
    header.writeUInt32LE(payload.length, 0);
    header.writeUInt32LE(32, 4);
    header.writeUInt16LE(0xffff, 8);
    header.writeUInt16LE(type, 10);
    header.writeUInt16LE(0xffff, 12);
    header.writeUInt16LE(name, 14);
    header.writeUInt16LE(language, 22);
    chunks.push(header, payload);
    const pad = (4 - (payload.length % 4)) % 4;
    if (pad) chunks.push(Buffer.alloc(pad));
  }
  writeFileSync(file, Buffer.concat(chunks));
}
