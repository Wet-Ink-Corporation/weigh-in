// Spike for #21: start a hook the way Claude Code 2.1.288 starts an exec-form hook
// (`child_process.spawn(command, args, {env, cwd, detached, windowsHide})` under Bun, the
// hook JSON written to stdin), and report what reached the process.
import { spawn } from "node:child_process";

const [command, ...args] = process.argv.slice(2);
const payload = JSON.stringify({ hook_event_name: "SessionStart", session_id: "spike" });
const runtime = typeof Bun !== "undefined" ? `bun ${Bun.version}` : `node ${process.version}`;

const child = spawn(command, args, {
  env: process.env,
  cwd: process.cwd(),
  detached: process.platform !== "win32",
  windowsHide: true,
});
let out = "";
let err = "";
child.stdout.on("data", (d) => (out += d));
child.stderr.on("data", (d) => (err += d));
child.on("error", (e) => {
  console.log(`RESULT ${runtime} command=${command} spawn-error=${e.code ?? ""} ${e.message}`);
  process.exitCode = 1;
});
child.on("close", (code) => {
  console.log(`RESULT ${runtime} command=${command} exit=${code} stdout=${JSON.stringify(out.trim())} stderr=${JSON.stringify(err.trim())}`);
  if (code !== 0 || !out.includes("spike")) process.exitCode = 1;
});
child.stdin.write(payload + "\n", "utf8");
child.stdin.end();
