import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

export function readExternal(dir, taskIds) {
  if (!existsSync(dir)) return [];
  const ids = new Set(taskIds), frames = [];
  const keys = ["schema", "task", "agent", "variant", "tokens_in", "tokens_out", "turns", "success"];
  for (const file of readdirSync(dir).filter((name) => name.endsWith(".jsonl")).sort()) {
    readFileSync(join(dir, file), "utf8").split("\n").forEach((line, i) => {
      if (!line.trim()) return;
      const fail = (reason) => { throw new Error(`${file}:${i + 1}: ${reason}`); };
      let data;
      try { data = JSON.parse(line); } catch { fail("not valid JSON"); }
      if (!data || typeof data !== "object" || Array.isArray(data)) fail('schema must be "tz-ai-external/1"');
      for (const key of keys) if (!(key in data)) fail(`missing key '${key}'`);
      if (data.schema !== "tz-ai-external/1") fail('schema must be "tz-ai-external/1"');
      if (!ids.has(data.task)) fail(`unknown task '${data.task}'`);
      for (const [key, pattern] of [
        ["agent", "^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$"],
        ["variant", "^[a-z0-9][a-z0-9._-]{0,31}$"],
      ]) if (typeof data[key] !== "string" || !new RegExp(pattern).test(data[key])) fail(`${key} must match ${pattern}`);
      for (const key of ["tokens_in", "tokens_out", "turns"]) {
        if (!Number.isSafeInteger(data[key]) || data[key] < 0) fail(`${key} must be a non-negative integer`);
      }
      if (typeof data.success !== "boolean") fail("success must be a boolean");
      frames.push(Object.fromEntries(keys.map((key) => [key, data[key]])));
    });
  }
  return frames;
}
