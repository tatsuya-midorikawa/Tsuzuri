import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { countText, estimateTokens, invoke } from "./measure.mjs";
import { prepareKb, resolveKbId } from "./kb.mjs";
import { mcpToolsList } from "./mcp.mjs";
import { copyProject } from "./corpus.mjs";

export const STATIC_ARTIFACTS = [
  { artifact: "guide-l0", args: ["guide"], limit: 6000 },
  { artifact: "guide-l1", args: ["guide", "--level", "1"], limit: 20000 },
  { artifact: "guide-std-array", args: ["guide", "std", "Array"], limit: 6000 },
  { artifact: "guide-std-all", args: ["guide", "--topic", "std-all"], limit: 45000 },
  { artifact: "agents-md", file: "AGENTS.md", limit: 3500 },
  { artifact: "llms-txt", file: "llms.txt", limit: 1500 },
  { artifact: "kb-brief-l0", args: ["kb", "brief", "--project"], limit: 2400 },
  { artifact: "kb-brief-l1", args: ["kb", "brief", "--module", "Orders"], limit: 1600 },
  { artifact: "kb-brief-l2", limit: 700 },
  { artifact: "mcp-tools-list", limit: null },
];
export async function measureStatic(ctx) {
  const rows = [];
  const created = invoke(ctx.compiler, ["new", "demo"], ctx.work).status === 0;
  const task = { corpus: ["benchmarks/ai/corpus/shop"] };
  let kbCwd;
  if (ctx.caps.kbBrief === null) kbCwd = prepareKb(ctx, task);
  for (const artifact of STATIC_ARTIFACTS) {
    let text;
    if (artifact.artifact === "mcp-tools-list") {
      const cwd = kbCwd ?? copyProject(ctx.tree.physical("benchmarks/ai/corpus/shop"), join(ctx.work, "mcp"));
      const measured = await mcpToolsList(ctx.compiler, cwd);
      if (measured) rows.push({ schema: "tz-ai-static/1", artifact: artifact.artifact,
        chars: measured.chars, est_tokens: measured.est_tokens, limit: null });
      else ctx.staticSkipped.push({ strategy: "mcp-tools-list", reason: "tsuzuri mcp did not answer tools/list" });
      continue;
    }
    if (artifact.file) {
      const path = join(ctx.work, "demo", artifact.file);
      if (!created || !existsSync(path)) continue;
      text = readFileSync(path, "utf8");
    } else {
      let args = artifact.args;
      const isKb = artifact.artifact.startsWith("kb-");
      if (isKb && !kbCwd) continue;
      if (!args) args = ["kb", "brief", resolveKbId(ctx, task, "Orders.order_total", kbCwd)];
      const result = invoke(ctx.compiler, args, isKb ? kbCwd : ctx.work);
      if (result.status !== 0) continue;
      text = result.stdout;
    }
    const count = countText(text);
    rows.push({ schema: "tz-ai-static/1", artifact: artifact.artifact, chars: count.chars,
      est_tokens: estimateTokens(count.ascii, count.other), limit: artifact.limit });
  }
  return rows;
}
