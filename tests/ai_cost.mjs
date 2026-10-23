import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { countText, estimateTokens, lowerMedian, Agent, invoke } from "../benchmarks/ai/lib/measure.mjs";
import { ROOT, Tree, copyProject } from "../benchmarks/ai/lib/corpus.mjs";
import { TYPES, loadTasks, resolveNeeds } from "../benchmarks/ai/lib/tasks.mjs";
import { stripComment, parseItems, deriveCallers, deriveImpact, itemRange } from "../benchmarks/ai/lib/derive.mjs";
import { STRATEGIES, mergeWindows, applyEdits, diagV1, diagV2, diagFix, firstError, hasError } from "../benchmarks/ai/lib/strategies.mjs";
import { makeRow, renderReport, serializeRows, updateReportDocument } from "../benchmarks/ai/lib/report.mjs";
import { readExternal } from "../benchmarks/ai/lib/external.mjs";
import { STATIC_ARTIFACTS, measureStatic } from "../benchmarks/ai/lib/static.mjs";
import { kb, kbSucceeded, resolveKbId } from "../benchmarks/ai/lib/kb.mjs";
import { mcpToolsList } from "../benchmarks/ai/lib/mcp.mjs";
import { detectCapabilities, parseArguments, verifyTask } from "../benchmarks/ai/run.mjs";

if (process.platform === "win32") {
  console.log("skip: G31 is not supported on Windows");
  process.exit(0);
}
const compiler = resolve(process.argv[2] ?? join(ROOT, "target/release/tsuzuri"));
const filter = process.argv[3] ?? "";
const scratch = join(ROOT, `target/scratch/ai-cost-test-${process.pid}`);
const tree = new Tree(), tasks = loadTasks(), task = (id) => tasks.find((task) => task.id === id);
const shop = "benchmarks/ai/corpus/shop";
let passed = 0, failed = 0, skipped = 0;
const tests = [];
const test = (name, fn) => tests.push([name, fn]);
const skip = (reason) => { skipped++; console.log(`skip: ${reason}`); };
function put(path, text) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
  return path;
}
function cli(args) {
  return spawnSync(process.execPath, [join(ROOT, "benchmarks/ai/run.mjs"), ...args], {
    cwd: ROOT, encoding: "utf8", timeout: 120000, maxBuffer: 16 * 1024 * 1024,
  });
}
function ok(result) { assert.equal(result.status, 0, result.stderr + result.stdout); return result.stdout; }
function context(name, binary = compiler) {
  const work = join(scratch, name);
  mkdirSync(work, { recursive: true });
  return { compiler: binary, tree, out: work, work, caps: {}, staticSkipped: [] };
}
function mutation(name, mutate) {
  const dir = join(scratch, name);
  copyProject(join(ROOT, "benchmarks/ai/tasks"), dir);
  const file = join(dir, "a-definition.json"), json = JSON.parse(readFileSync(file));
  mutate(json.tasks[0]);
  put(file, JSON.stringify(json));
  return dir;
}
// These numbers are the independent shell totals in appendix D, not harness output.
const GOLD = {
  "A-05": [[1048, 2, 262], [107, 1, 27]],
  "B-05": [[2567, 3, 642], [2567, 3, 642]],
  "B-06": [[1248, 2, 312], [478, 1, 120]],
  "F-01": [[3611, 8, 903], [2381, 6, 596]],
  "F-02": [[8882, 16, 2221], [7652, 14, 1913]],
  "F-03": [[6362, 12, 1591], [5902, 11, 1476]],
  "F-04": [[4320, 7, 1080], [3860, 6, 965]],
  "F-05": [[2708, 3, 677], [358, 1, 90]],
  "F-06": [[4772, 9, 1193], [4312, 8, 1078]],
};
test("estimate_tokens_matches_the_formula", () => {
  for (const [a, o, expected] of [[0,0,0],[1,0,1],[4,0,1],[5,0,2],[0,1,2],[1,1,2],[0,10,13],[3999,0,1000],[4001,0,1001]]) {
    assert.equal(estimateTokens(a, o), expected);
  }
  for (let a = 0; a <= 40; a++) for (let o = 0; o <= 40; o++) {
    assert.equal(estimateTokens(a, o), Math.ceil((10 * a + 52 * o) / 40));
  }
});
test("chars_count_code_points_not_utf16_units", () => {
  assert.deepEqual(countText("a😀あ"), { chars: 3, ascii: 1, other: 2 });
  assert.deepEqual(countText(""), { chars: 0, ascii: 0, other: 0 });
  const agent = new Agent(tree);
  for (let i = 0; i < 4; i++) agent.emit("empty", "a");
  assert.equal(agent.tokens, 1);
});
test("lower_median_picks_the_lower_middle", () => {
  assert.equal(lowerMedian([5,1,3]), 3);
  assert.equal(lowerMedian([4,1,3,2]), 2);
  assert.equal(lowerMedian([7]), 7);
  assert.throws(() => lowerMedian([]));
});
test("files_are_sorted_and_filtered", () => {
  const dir = join(scratch, "tree");
  for (const name of ["b.tz","a.md",".hidden.tz","target/x.tz","node_modules/y.tz","c.txt","sub/d.tc"]) put(join(dir, name), "");
  const t = new Tree(dir);
  assert.deepEqual(t.files("."), ["a.md","b.tz","sub/d.tc"]);
  assert.deepEqual(t.files("b.tz"), ["b.tz"]);
  assert.throws(() => t.files("nope"), /corpus root 'nope' does not exist/);
  assert.throws(() => t.lines("../outside"), /invalid logical path/);
});
test("lines_drop_one_trailing_empty_element", () => {
  for (const [i, text, lines] of [[0,"a\nb\n",["a","b"]],[1,"a\nb",["a","b"]],[2,"a\n\n",["a",""]],[3,"",[]]]) {
    const dir = join(scratch, `lines-${i}`);
    put(join(dir, "x.tz"), text);
    assert.deepEqual(new Tree(dir).lines("x.tz"), lines);
  }
});
test("grep_formats_orders_and_caps", () => {
  const agent = new Agent(tree), result = agent.grep("percent_of", [shop]);
  assert.equal(countText(result).chars, 478);
  assert.equal(result.trim().split("\n").length, 4);
  assert.ok(result.startsWith(`${shop}/Money.tz:2:def percent_of ::`));
  const dir = join(scratch, "grep"); put(join(dir, "x.tz"), "match\n".repeat(150));
  const a = new Agent(new Tree(dir)); a.grep("match", ["."]);
  assert.equal(a.seen.get("x.tz").size, 100);
  assert.equal(a.grep("absent", ["."]), ""); assert.equal(a.calls, 2);
});
test("read_clamps_and_marks_lines", () => {
  const dir = join(scratch, "read"); put(join(dir, "x.tz"), Array.from({ length: 1200 }, (_, i) => `${i + 1}`).join("\n") + "\n");
  const a = new Agent(new Tree(dir));
  assert.equal(a.read("x.tz", -5, 3), "1\n2\n3\n");
  assert.equal(a.read("x.tz", 1200, 1500), "1200\n");
  assert.equal(a.chunk("x.tz", 2).split("\n")[0], "501");
  assert.ok(a.covers({ path: "x.tz", from: 501, to: 1000 }));
  assert.throws(() => a.read("x.tz", 1201, 1300), /empty read/);
});
test("needs_must_match_exactly_one_line", () => {
  const dir = join(scratch, "needs"); put(join(dir, "x.tz"), "same\nsame\nunique\n");
  const t = new Tree(dir);
  assert.throws(() => resolveNeeds([["x.tz","same",0,0]], t), /2 matching lines/);
  assert.throws(() => resolveNeeds([["x.tz","missing",0,0]], t), /0 matching lines/);
  assert.deepEqual(resolveNeeds([["x.tz","unique",20,20]], t), [{ path: "x.tz", from: 1, to: 3 }]);
});
test("task_files_are_valid", () => {
  assert.equal(tasks.length, 37);
  assert.deepEqual(tasks.map((t) => t.id), tasks.map((t) => t.id).sort());
  assert.equal(new Set(tasks.map((t) => t.id)).size, 37);
  for (const type of Object.values(TYPES)) assert.ok(tasks.filter((t) => t.type === type.name).length >= 6);
});
test("unknown_and_missing_task_keys_are_rejected", () => {
  assert.throws(() => loadTasks(mutation("unknown", (t) => { t.foo = 1; })), /task A-01: unknown key 'foo'/);
  assert.throws(() => loadTasks(mutation("missing", (t) => { delete t.grep; })), /task A-01: missing key 'grep'/);
  assert.throws(() => loadTasks(mutation("bad-path", (t) => { t.corpus = ["../outside"]; })), /invalid logical path/);
  assert.throws(() => loadTasks(mutation("bad-need", (t) => { t.needs[0][2] = -1; })), /invalid need/);
});
test("items_follow_the_keyword_rule", () => {
  const dir = join(scratch, "items");
  put(join(dir, "X.tz"), 'private def rec f :: i64 -> i64\nfn rec f x = x // comment\nand g y = f y\n\ntest "a // b" =\n    def nested = 1\nrecord R { x: i64 }\n');
  const items = parseItems(["."], new Tree(dir));
  assert.deepEqual(items.map((i) => [i.kind,i.id,i.from,i.to]), [
    ["function","X.f",1,1],["function","X.f",2,2],["function","X.g",3,3],["test","X.a // b",5,6],
  ]);
  assert.equal(stripComment('"a \\" // b" // c'), '"a \\" // b" ');
  assert.deepEqual(itemRange(new Tree(dir).lines("X.tz"), 6), [5,6]);
});
test("callers_and_impact_match_expect_on_shop", () => {
  for (const t of tasks.filter((t) => t.expect && t.corpus?.includes(shop))) {
    assert.deepEqual(t.type === "callers" ? deriveCallers(t, tree).callers : deriveImpact(t, tree).tests, t.expect);
  }
  assert.equal(deriveImpact(task("F-02"), tree).grep.join(","), "percent_of,with_discount,with_tax,total_with_discount,gross,discounted,price_of,main,line_total,order_total,summary");
});
test("compiler_test_list_matches_parsed_tests", () => {
  const result = invoke(compiler, ["test", ".", "--list"], tree.physical(shop));
  assert.equal(result.status, 0, result.stderr);
  const listed = result.stdout.trim().split("\n").map((line) => line.replace(/^\d+\s+/, "")).sort();
  assert.equal(listed.length, 16);
  assert.deepEqual(listed, parseItems([shop], tree).filter((i) => i.kind === "test").map((i) => i.id).sort());
  assert.equal(invoke(compiler, ["check", "."], tree.physical(shop)).stderr, "");
});
test("shop_goldens_for_the_grep_baselines", () => {
  for (const id of Object.keys(GOLD)) for (const [i, strategy] of STRATEGIES.slice(0,3).entries()) {
    const outcome = strategy.run({ tree }, task(id)), a = outcome.agent;
    assert.deepEqual([a.chars,a.calls,a.tokens], GOLD[id][i === 0 ? 0 : 1], `${id}/${strategy.name}`);
    assert.equal(outcome.complete, true);
  }
  const shell = spawnSync("sh", ["benchmarks/ai/golden.sh"], { cwd: ROOT, encoding: "utf8" });
  assert.equal(shell.status, 0, shell.stderr);
  for (const line of shell.stdout.trim().split("\n")) {
    const [id, ...values] = line.split(" ");
    assert.deepEqual(values.map(Number), GOLD[id][1]);
  }
});
test("chunks_are_read_until_the_needs_are_covered", () => {
  const dir = join(scratch, "chunks");
  put(join(dir,"x.tz"), Array.from({ length: 1200 }, (_, i) => `line-${i + 1}:`).join("\n") + "\n");
  for (const [line, calls] of [[100,2],[1100,4]]) {
    const t = { type: "definition", corpus: ["."], grep: ["absent"], needs: [["x.tz",`line-${line}:`,0,0]] };
    const result = STRATEGIES[1].run({ tree: new Tree(dir) }, t);
    assert.equal(result.agent.calls, calls); assert.equal(result.complete, true);
  }
});
test("windows_merge_adjacent_ranges", () => {
  assert.deepEqual(mergeWindows([[35,50],[1,10],[11,20],[30,40]]), [[1,20],[30,50]]);
  assert.deepEqual(mergeWindows([[5,5]]), [[5,5]]);
});
test("docs_strategy_reads_the_index_first", () => {
  const t = new Tree(), calls = [], original = t.lines.bind(t);
  t.lines = (path) => { calls.push(path); return original(path); };
  const outcome = STRATEGIES[3].run({ tree: t }, task("A-02"));
  assert.equal(calls[0], "_tsuzuri/language-reference/index.md");
  assert.equal(outcome.agent.calls, 2);
});

let results, meta, firstOut;
function runTwice() {
  if (results) return;
  const a = join(scratch, "run-a"), b = join(scratch, "run-b");
  for (const dir of [a,b]) ok(cli(["run","--compiler",compiler,"--out",dir]));
  for (const file of ["results.jsonl","static.jsonl","meta.json"]) assert.equal(readFileSync(join(a,file),"utf8"),readFileSync(join(b,file),"utf8"), file);
  firstOut = a;
  results = readFileSync(join(a,"results.jsonl"),"utf8").trim().split("\n").map(JSON.parse);
  meta = JSON.parse(readFileSync(join(a,"meta.json")));
}
test("run_is_deterministic", () => {
  runTwice(); assert.equal(results.length, meta.rows);
  assert.ok(results.length >= 104); assert.equal(meta.skipped_tasks.length, 0);
  assert.ok(!existsSync(join(firstOut,"work")));
  assert.ok(!/\/Users\/|\/home\/|timestamp|hostname/.test(serializeRows(results)));
  const phase1 = results.filter((row) => row.type !== "diagnostic" && row.strategy.startsWith("baseline-"));
  assert.equal(phase1.length, 97);
});
test("result_rows_have_the_documented_keys", () => {
  runTwice();
  assert.deepEqual(Object.keys(results[0]), ["schema","task","type","strategy","chars","calls","est_tokens","complete"]);
  for (const row of results) for (const key of ["chars","calls","est_tokens"]) assert.ok(Number.isSafeInteger(row[key]));
});
test("cli_usage_errors_exit_two", () => {
  for (const [args, message] of [
    [[], "unknown command ''"], [["other"],"unknown command 'other'"], [["run","--wat"],"unknown option '--wat'"],
    [["run","--types","Z"],"--types needs letters from A to F"], [["run","--strategies","wat"],"unknown strategy 'wat'"],
    [["task","Z-99"],"unknown task 'Z-99'"], [["run","--out"],"needs a value"],
    [["run","--out","x","--out","y"],"duplicate option"],
  ]) { const result = cli(args); assert.equal(result.status,2,result.stderr); assert.ok(result.stderr.includes(message)); assert.ok(result.stderr.includes("usage:")); }
  assert.deepEqual(parseArguments(["run","--types","a,c"]).types, ["A","C"]);
});
test("task_command_hides_the_answer", () => {
  assert.deepEqual(Object.keys(JSON.parse(ok(cli(["task","A-01"])))), ["id","type","prompt","corpus"]);
  assert.deepEqual(Object.keys(JSON.parse(ok(cli(["task","C-01"])))), ["id","type","prompt","project"]);
});
test("verify_passes_on_the_repository_tasks", () => {
  assert.ok(ok(cli(["verify","--compiler",compiler,"--strict"])).endsWith("verified 37 tasks: 0 failures, 0 drift\n"));
});
test("verify_fails_on_a_broken_stable_task", () => {
  const dir = mutation("broken", () => {});
  const file = join(dir,"b-callers.json"), json = JSON.parse(readFileSync(file));
  json.tasks.find((t) => t.id === "B-05").expect.pop(); put(file,JSON.stringify(json));
  const result = cli(["verify","--compiler",compiler,"--tasks",dir]);
  assert.equal(result.status,1); assert.ok(result.stdout.includes("task B-05: derived callers differ from expect"));
});
test("report_tables_are_exact", () => {
  const row = (id, n, complete) => ({ schema: "tz-ai-result/1", task: id, type: "definition",
    strategy: "baseline-grep", chars: n, calls: n, est_tokens: n, complete });
  const text = renderReport({ rows: [row("A-01",4,true),row("A-02",1,true),row("A-03",3,false),row("A-04",2,true)],
    meta: { compiler: "tsuzuri 0.1.0", commit: "abc", dirty: true }, taskCount: 4 });
  assert.equal(text, "測定: tsuzuri 0.1.0、コミット abc（未コミットの変更あり）、タスク 4 件、行 4\n\n" +
    "### 型ごとの値（下位の中央値）\n\n| 型 | 戦略 | 件数 | chars | calls | est_tokens | complete |\n" +
    "| --- | --- | --- | --- | --- | --- | --- |\n| definition | baseline-grep | 4 | 2 | 2 | 2 | 3/4 |\n");
});
test("report_replaces_only_the_marked_region", () => {
  const doc = "before\n<!-- generated:ai-cost -->\nold\n<!-- /generated:ai-cost -->\nafter\n";
  const next = updateReportDocument(doc,"new\n");
  assert.equal(next,doc.replace("old","new"));
  assert.equal(updateReportDocument(next,"new\n"),next);
  for (const invalid of ["",doc+doc,"<!-- /generated:ai-cost -->\n<!-- generated:ai-cost -->"]) assert.throws(() => updateReportDocument(invalid,"new\n"));
});
test("report_publish_updates_the_document", () => {
  runTwice();
  const doc = put(join(scratch,"report.md"), "before\n<!-- generated:ai-cost -->\n<!-- /generated:ai-cost -->\nafter\n");
  ok(cli(["report","--out",firstOut,"--publish","--doc",doc]));
  const updated = readFileSync(doc,"utf8");
  assert.equal(updated,updateReportDocument("before\n<!-- generated:ai-cost -->\n<!-- /generated:ai-cost -->\nafter\n",ok(cli(["report","--out",firstOut]))));
  ok(cli(["report","--out",firstOut,"--publish","--doc",doc]));
  assert.equal(readFileSync(doc,"utf8"),updated);
});
test("diag_corpus_reports_the_expected_first_diagnostic", () => {
  for (const t of tasks.filter((t) => t.type === "diagnostic")) {
    const d = firstError(new Agent(new Tree(tree.physical(t.project)),compiler).check(tree.physical(t.project)));
    assert.deepEqual([d.code,d.span.line],[t.expect.code,t.expect.line]);
  }
});
test("diag_patches_behave", () => {
  const ctx = context("patches");
  for (const t of tasks.filter((t) => t.type === "diagnostic")) verifyTask(t,ctx);
  const dir = context("literal-edit").work;
  put(join(dir,"Main.tz"),"find\n"); applyEdits(dir,[{ path:"Main.tz",find:"find",replace:"$&$1" }]);
  assert.equal(readFileSync(join(dir,"Main.tz"),"utf8"),"$&$1\n");
  assert.throws(() => applyEdits(dir,[{ path:"Main.tz",find:"absent",replace:"" }]),/must occur exactly once/);
  assert.throws(() => hasError("not JSON"),/non-JSON diagnostic/);
});
test("diag_v1_counts_calls_and_rounds", () => {
  for (const [id,calls,rounds] of [["C-01",6,2],["C-02",4,1],["C-03",6,2],["C-04",6,2],["C-05",6,2],["C-06",4,1],["C-07",4,1]]) {
    const result = diagV1(context(`diag-${id}`),task(id));
    assert.deepEqual([result.agent.calls,result.rounds,result.complete],[calls,rounds,true]);
  }
});
test("diag_rows_have_rounds_and_code", () => {
  const t = task("C-02"), row = makeRow(t,{name:"diag-v1"},diagV1(context("diag-row"),t));
  assert.deepEqual(Object.keys(row),["schema","task","type","strategy","chars","calls","est_tokens","rounds","code","complete"]);
  assert.equal(row.code,"E1014"); assert.equal(row.complete,true);
});

let mockCompiler;
function mock() {
  if (mockCompiler) return mockCompiler;
  mockCompiler = put(join(scratch,"compiler.mjs"), `#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline";
const a = process.argv.slice(2), real = ${JSON.stringify(compiler)};
if (a[0] === "check") {
  const r = spawnSync(real,a,{encoding:"utf8"});
  process.stdout.write(r.stdout);
  for (const line of r.stderr.split("\\n").filter(Boolean)) {
    const d = JSON.parse(line);
    d.related = [{ path:"./Main.tz", span:{line:6,end_line:6}, role:"declared_here", message:"fixture" }];
    process.stderr.write(JSON.stringify(d)+"\\n");
  }
  process.exit(r.status);
}
if (a[0] === "--version") { console.log("tsuzuri fixture"); process.exit(0); }
if (a[0] === "fix") {
  if (a.length === 1) { console.error("usage: fix PATH"); process.exit(2); }
  const s = readFileSync("Main.tz","utf8");
  writeFileSync("Main.tz",s.replace("    | Triangle _ -> 3","    | Triangle _ -> 3\\n    | Square _ -> 4"));
  const r = spawnSync(real,["check","."],{encoding:"utf8"});
  console.log("applied 1 fix in 1 file"); process.stderr.write(r.stderr); process.exit(r.status);
}
if (a[0] === "guide") { console.log("guide fixture"); process.exit(0); }
if (a[0] === "new") {
  mkdirSync(a[1],{recursive:true}); writeFileSync(a[1]+"/AGENTS.md","agents\\n"); writeFileSync(a[1]+"/llms.txt","llms\\n"); process.exit(0);
}
if (a[0] === "kb") {
  if (a.length === 2 && a[1] !== "build") { console.error("usage: kb SUBCOMMAND ARG"); process.exit(2); }
  if (a[1] === "export") { for(const id of ["Orders.order_total","Catalog.Item"]) console.log(JSON.stringify({id:"Shop::"+id})); }
  else if (a[1] === "callers") console.log("Orders.total_with_discount Report.summary");
  else if (a[1] === "brief") console.log("brief");
  else if (a[1] === "show") console.log("i64 -> i64");
  else if (a[1] === "find") console.log(a[2]);
  else if (a[1] === "tests-for") console.log("Stock.take reduces the count");
  process.exit(0);
}
if (a[0] === "mcp") {
  createInterface({input:process.stdin}).on("line",line=>{
    const x=JSON.parse(line);
    if(x.id === 1) console.log(JSON.stringify({jsonrpc:"2.0",id:1,result:{protocolVersion:"2025-06-18"}}));
    if(x.id === 2) console.log(JSON.stringify({jsonrpc:"2.0",id:2,result:{tools:[{name:"show",description:"fixture"}]}}));
  });
} else process.exit(1);
`);
  chmodSync(mockCompiler,0o755);
  return mockCompiler;
}
test("diag_v2_is_skipped_or_reads_related", () => {
  const ctx = context("v2-real"); ctx.caps = detectCapabilities(ctx);
  if (ctx.caps.diagV2) skip(ctx.caps.diagV2);
  else assert.equal(diagV2(ctx,task("C-01")).complete,true);
  const fixture = context("v2-fixture",mock()), t = task("C-01");
  const v2 = diagV2(fixture,t), v1 = diagV1(context("v1-fixture",mock()),t);
  assert.equal(v2.complete,true); assert.ok(v2.agent.chars < v1.agent.chars);
  assert.deepEqual([...v2.agent.seen.get("Main.tz")].slice(0,3),[1,2,3]);
});
test("diag_fix_is_skipped_or_finishes_in_two_calls", () => {
  const ctx = context("fix-real"); ctx.caps = detectCapabilities(ctx);
  if (ctx.caps.diagFix) skip(ctx.caps.diagFix);
  else assert.equal(diagFix(ctx,task("C-06")).complete,true);
  const result = diagFix(context("fix-fixture",mock()),task("C-06"));
  assert.deepEqual([result.agent.calls,result.rounds,result.complete],[2,0,true]);
  const fallback = diagFix(context("fix-fallback",mock()),task("C-01"));
  assert.equal(fallback.complete,true); assert.ok(fallback.agent.calls > 2);
});
test("unavailable_strategy_is_an_error", () => {
  runTwice();
  const names = ["diag-v2","diag-fix","kb","kb-oriented"].filter((name) => meta.skipped.some((s) => s.strategy === name));
  if (!names.length) skip("all optional strategies are available");
  for (const name of names) {
    const result = cli(["run","--compiler",compiler,"--out",join(scratch,`unavailable-${name}`),"--strategies",name]);
    assert.equal(result.status,1); assert.ok(result.stderr.includes(`strategy '${name}' is not available:`));
  }
});
const frame = { schema:"tz-ai-external/1", task:"B-05", agent:"agent", variant:"plain", tokens_in:12, tokens_out:3, turns:4, success:true };
test("external_frames_are_validated", () => {
  const dir = join(scratch,"external");
  const file = put(join(dir,"a.jsonl"),JSON.stringify({...frame,ignored:"secret"})+"\n\n");
  assert.deepEqual(readExternal(dir,tasks.map((t)=>t.id)),[frame]);
  for (const [edit,message] of [
    [(f)=>{f.schema="wrong";},'schema must be "tz-ai-external/1"'],[(f)=>{delete f.agent;},"missing key 'agent'"],
    [(f)=>{f.task="Z-99";},"unknown task 'Z-99'"],[(f)=>{f.agent="a/b";},"agent must match"],
    [(f)=>{f.variant="Upper";},"variant must match"],[(f)=>{f.tokens_in=-1;},"tokens_in must be a non-negative integer"],
    [(f)=>{f.tokens_out=Number.MAX_SAFE_INTEGER+1;},"tokens_out must be a non-negative integer"],
    [(f)=>{f.turns=0.5;},"turns must be a non-negative integer"],[(f)=>{f.success=1;},"success must be a boolean"],
  ]) { const f={...frame}; edit(f); put(file,JSON.stringify(f)); assert.throws(()=>readExternal(dir,["B-05"]),new RegExp(message.replace(/[.*+?^$()|[\]\\]/g,"\\$&"))); }
  put(file,"{"); assert.throws(()=>readExternal(dir,["B-05"]),/a.jsonl:1: not valid JSON/);
  runTwice(); const live=put(join(firstOut,"external","a.jsonl"),JSON.stringify(frame));
  assert.ok(ok(cli(["report","--out",firstOut])).includes("| agent | plain | callers | 1 | 1/1 | 12 | 3 | 4 |"));
  put(live,"{"); assert.equal(cli(["report","--out",firstOut]).status,1); rmSync(live);
});
test("external_table_uses_lower_medians", () => {
  const text=renderReport({rows:[],meta:{compiler:"x",commit:"abc"},externals:[4,1,3,2].map((n)=>({...frame,tokens_in:n,tokens_out:n,turns:n,success:n!==4}))});
  assert.ok(text.includes("| agent | plain | callers | 4 | 3/4 | 2 | 2 | 2 |"));
});
test("static_artifact_limits", async () => {
  const ctx=context("static-fixture",mock()); ctx.caps=detectCapabilities(ctx);
  const rows=await measureStatic(ctx);
  assert.equal(rows.length,10);
  assert.deepEqual(rows.map((r)=>r.limit),[6000,20000,6000,45000,3500,1500,2400,1600,700,null]);
  for(const row of rows) if(row.limit!==null) assert.ok(row.chars<=row.limit);
  runTwice();
  const real=readFileSync(join(firstOut,"static.jsonl"),"utf8").split("\n").filter(Boolean).map(JSON.parse);
  if(!real.length) skip("G21/G27/G28 static artifacts unavailable");
  for(const row of real) if(row.limit!==null) assert.ok(row.chars<=row.limit);
  assert.equal(STATIC_ARTIFACTS.length,10);
});
test("static_rows_are_deterministic", async () => {
  const measure=async(name)=>{const ctx=context(name,mock());ctx.caps=detectCapabilities(ctx);return measureStatic(ctx);};
  assert.deepEqual(await measure("static-a"),await measure("static-b"));
});
test("kb_strategies_are_skipped_or_use_one_call", () => {
  const fixture=context("kb-fixture",mock());
  assert.deepEqual([kb(fixture,task("B-05")).agent.calls,kb(fixture,task("B-05"),true).agent.calls],[1,2]);
  assert.equal(kb(fixture,task("B-05")).complete,true);
  assert.equal(resolveKbId(fixture,task("A-05"),"Catalog.Item"),"Shop::Catalog.Item");
  assert.throws(()=>resolveKbId(fixture,task("A-05"),"Missing.x"),/0 matches/);
  runTwice();
  const rows=results.filter((r)=>r.task==="B-05"&&r.strategy.startsWith("kb"));
  if(!rows.length) skip("G27/G28 kb commands unavailable");
  for(const row of rows) assert.equal(row.calls,row.strategy==="kb"?1:2);
});
test("kb_success_uses_substring_checks", () => {
  assert.equal(kbSucceeded(task("A-01"),task("A-01").answer[0]),true);
  assert.equal(kbSucceeded(task("B-05"),"Shop::Orders.total_with_discount\nShop::Report.summary"),true);
  assert.equal(kbSucceeded(task("B-05"),"Orders.total_with_discount"),false);
  assert.equal(kbSucceeded({...task("A-01"),kb:{contains:["one","two"]}},"one two"),true);
  assert.equal(kbSucceeded({...task("A-01"),kb:{contains:["one","two"]}},"one"),false);
  assert.equal(kbSucceeded(task("F-05"),"test:Stock.take reduces the count"),true);
});
test("mcp_tools_list_row_or_skip", async () => {
  const expected=JSON.stringify([{name:"show",description:"fixture"}]), count=countText(expected);
  assert.deepEqual(await mcpToolsList(mock(),scratch),{chars:count.chars,est_tokens:estimateTokens(count.ascii,count.other),tools:1});
  const invalid=put(join(scratch,"invalid.mjs"),'#!/usr/bin/env node\nconsole.log("not JSON");\n');
  chmodSync(invalid,0o755); assert.equal(await mcpToolsList(invalid,scratch),null);
  runTwice();
  if(meta.skipped.some((s)=>s.strategy==="mcp-tools-list")) skip("G29 MCP tools/list unavailable");
  else assert.ok(readFileSync(join(firstOut,"static.jsonl"),"utf8").includes("mcp-tools-list"));
});

mkdirSync(scratch,{recursive:true});
try {
  for (const [name,fn] of tests) if(name.includes(filter)) {
    try { await fn(); passed++; console.log(`ok ${name}`); }
    catch(error) { failed++; console.error(`not ok ${name}: ${error.stack}`); }
  }
  if(!passed&&!failed) { failed++; console.error(`no tests matched '${filter}'`); }
} finally { rmSync(scratch,{recursive:true,force:true}); }
console.log(`ai_cost: ${passed} passed, ${failed} failed, ${skipped} skipped`);
process.exitCode=failed?1:0;
