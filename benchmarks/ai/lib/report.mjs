import { lowerMedian } from "./measure.mjs";
import { TYPES } from "./tasks.mjs";
import { STRATEGIES } from "./strategies.mjs";

export function makeRow(task, strategy, outcome) {
  const row = { schema: "tz-ai-result/1", task: task.id, type: task.type, strategy: strategy.name,
    chars: outcome.agent.chars, calls: outcome.agent.calls, est_tokens: outcome.agent.tokens };
  if (task.type === "diagnostic") {
    row.rounds = outcome.rounds;
    row.code = outcome.code;
  }
  row.complete = outcome.complete;
  return row;
}
export const serializeRows = (rows) => rows.map((row) => JSON.stringify(row) + "\n").join("");
const cell = (value) => String(value).replaceAll("\\", "\\\\").replaceAll("|", "\\|").replaceAll("\r", "").replaceAll("\n", " ");
function table(title, headings, rows) {
  if (!rows.length) return "";
  const line = (values) => `| ${values.map(cell).join(" | ")} |`;
  return `### ${title}\n\n${line(headings)}\n${line(headings.map(() => "---"))}\n${rows.map(line).join("\n")}`;
}
function grouped(values, key) {
  const groups = new Map();
  for (const value of values) {
    const id = key(value);
    if (!groups.has(id)) groups.set(id, []);
    groups.get(id).push(value);
  }
  return groups;
}
export function renderReport({ rows, statics = [], meta, externals = [], taskCount = 37 }) {
  const strategies = STRATEGIES.map((strategy) => strategy.name), types = Object.values(TYPES).map((type) => type.name);
  for (const row of rows) {
    if (row.schema !== "tz-ai-result/1" || !types.includes(row.type) || !strategies.includes(row.strategy) ||
        !/^[A-F]-[0-9]{2}$/.test(row.task) || typeof row.complete !== "boolean" ||
        ["chars", "calls", "est_tokens"].some((key) => !Number.isSafeInteger(row[key]) || row[key] < 0) ||
        (row.type === "diagnostic" && (!/^E[0-9]{4}$/.test(row.code) || !Number.isSafeInteger(row.rounds) || row.rounds < 0))) {
      throw new Error("invalid tz-ai-result/1 row");
    }
  }
  const medians = (group, keys) => keys.map((key) => lowerMedian(group.map((row) => row[key])));
  const values = [];
  for (const type of types) for (const strategy of strategies) {
    const group = rows.filter((row) => row.type === type && row.strategy === strategy);
    if (group.length) values.push([type, strategy, group.length, ...medians(group, ["chars", "calls", "est_tokens"]),
      `${group.filter((row) => row.complete).length}/${group.length}`]);
  }
  const diagnostics = [...rows].filter((row) => row.type === "diagnostic").sort((a, b) =>
    a.task < b.task ? -1 : a.task > b.task ? 1 : strategies.indexOf(a.strategy) - strategies.indexOf(b.strategy));
  const externalGroups = grouped(externals, (frame) => JSON.stringify([frame.agent, frame.variant, frame.task[0]]));
  const externalRows = [...externalGroups.keys()].sort().map((key) => {
    const [agent, variant, letter] = JSON.parse(key), group = externalGroups.get(key);
    return [agent, variant, TYPES[letter].name, group.length, `${group.filter((frame) => frame.success).length}/${group.length}`,
      ...medians(group, ["tokens_in", "tokens_out", "turns"])];
  });
  const skipped = [
    ...(meta.skipped ?? []).map(({ strategy, reason }) => `- 戦略 \`${cell(strategy)}\`: ${cell(reason)}`),
    ...(meta.skipped_tasks ?? []).map(({ task, reason }) => `- タスク ${cell(task)}: ${cell(reason)}`),
  ];
  return [
    `測定: ${cell(meta.compiler)}、コミット ${cell(meta.commit)}${meta.dirty ? "（未コミットの変更あり）" : ""}、タスク ${taskCount} 件、行 ${rows.length}`,
    table("型ごとの値（下位の中央値）", ["型", "戦略", "件数", "chars", "calls", "est_tokens", "complete"], values),
    table("診断の修復（C）のタスクごとの値", ["タスク", "コード", "戦略", "chars", "calls", "rounds", "est_tokens"],
      diagnostics.map((row) => [row.task, row.code, row.strategy, row.chars, row.calls, row.rounds, row.est_tokens])),
    table("静的な成果物の大きさ", ["成果物", "chars", "est_tokens", "上限"],
      statics.map((row) => [row.artifact, row.chars, row.est_tokens, row.limit ?? "-"])),
    table("外部エージェントの実測（下位の中央値）", ["agent", "variant", "型", "件数", "成功", "tokens_in", "tokens_out", "turns"], externalRows),
    skipped.length ? `### 測らなかったもの\n\n${skipped.join("\n")}` : "",
  ].filter(Boolean).join("\n\n") + "\n";
}
export function updateReportDocument(document, generated) {
  const start = "<!-- generated:ai-cost -->", end = "<!-- /generated:ai-cost -->";
  const a = document.indexOf(start), b = document.indexOf(end);
  if (document.split(start).length !== 2 || document.split(end).length !== 2 || a >= b) {
    throw new Error("docs/ai-cost.md must contain exactly one <!-- generated:ai-cost --> before one <!-- /generated:ai-cost -->");
  }
  return document.slice(0, a + start.length) + "\n" + generated.trimEnd() + "\n" + document.slice(b);
}
