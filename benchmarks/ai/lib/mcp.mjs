import { spawn } from "node:child_process";
import { countText, estimateTokens } from "./measure.mjs";

export function mcpToolsList(compiler, cwd) {
  return new Promise((resolve) => {
    const child = spawn(compiler, ["mcp"], { cwd, stdio: ["pipe", "pipe", "ignore"] });
    let buffer = "", bytes = 0, expected = 1, done = false, result = null, timer, killTimer;
    const finish = (value) => {
      if (done) return;
      done = true;
      result = value;
      clearTimeout(timer);
      child.stdin.end();
      child.kill();
      killTimer = setTimeout(() => child.kill("SIGKILL"), 1000);
      killTimer.unref();
    };
    const send = (message) => child.stdin.write(JSON.stringify(message) + "\n");
    const deadline = () => {
      clearTimeout(timer);
      timer = setTimeout(() => finish(null), 10000);
    };
    child.on("error", () => { clearTimeout(timer); clearTimeout(killTimer); resolve(null); });
    child.on("close", () => {
      clearTimeout(timer);
      clearTimeout(killTimer);
      resolve(result);
    });
    child.stdin.on("error", () => finish(null));
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (text) => {
      if (done) return;
      bytes += Buffer.byteLength(text);
      if (bytes > 16 * 1024 * 1024) return finish(null);
      buffer += text;
      for (let at; !done && (at = buffer.indexOf("\n")) >= 0;) {
        const line = buffer.slice(0, at);
        buffer = buffer.slice(at + 1);
        if (!line.trim()) continue;
        let response;
        try { response = JSON.parse(line); } catch { return finish(null); }
        if (response?.jsonrpc !== "2.0") return finish(null);
        if (response.id !== expected) continue;
        if (response.error || !response.result) return finish(null);
        if (expected === 1) {
          if (response.result.protocolVersion !== "2025-06-18") return finish(null);
          expected = 2;
          send({ jsonrpc: "2.0", method: "notifications/initialized" });
          send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
          deadline();
        } else {
          if (!Array.isArray(response.result.tools)) return finish(null);
          try {
            const count = countText(JSON.stringify(response.result.tools));
            finish({ chars: count.chars, est_tokens: estimateTokens(count.ascii, count.other), tools: response.result.tools.length });
          } catch { finish(null); }
        }
      }
    });
    deadline();
    send({ jsonrpc: "2.0", id: 1, method: "initialize", params: {
      protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "tz-ai-cost", version: "1" },
    } });
  });
}
