import { exec, execFile } from "child_process";
import * as cp from "node:child_process";
import crypto from "crypto";

export function evals(userInput: string, n: number) {
  // ruleid: pmx-js-eval
  eval(userInput);
  // ok: pmx-js-eval
  eval("1 + 1");
  // ruleid: pmx-js-eval
  const f = new Function("a", userInput);
  // ok: pmx-js-eval
  const g = new Function("a", "return a * 2");
  // ruleid: pmx-js-eval
  setTimeout("run(" + n + ")", 10);
  // ok: pmx-js-eval
  setTimeout(() => run(n), 10);
  return [f, g];
}

export function commands(name: string) {
  // ruleid: pmx-js-command-injection
  exec(`git log ${name}`);
  // ruleid: pmx-js-command-injection
  exec("ls " + name);
  // ok: pmx-js-command-injection
  exec("git status");
  // ok: pmx-js-command-injection
  execFile("git", ["log", name]);
  // ok: pmx-js-command-injection
  /a+/.exec(name);
}

export function commands2(name: string) {
  const childProcess = require("child_process");
  // ruleid: pmx-js-command-injection
  childProcess.execSync("rm -rf " + name);
  // ok: pmx-js-command-injection
  childProcess.execSync("ls");
}

export async function sql(db: any, prisma: any, id: string) {
  // ruleid: pmx-js-sql-injection
  await db.query("SELECT * FROM users WHERE id = " + id);
  // ruleid: pmx-js-sql-injection
  await db.query(`SELECT * FROM users WHERE id = ${id}`);
  // ruleid: pmx-js-sql-injection
  await db.query("SELECT * FROM users WHERE name = '" + id + "'");
  // ruleid: pmx-js-sql-injection
  await db.query(" UPDATE users SET name = '" + id + "'");
  // ok: pmx-js-sql-injection
  await db.query("SELECT * FROM users WHERE id = $1", [id]);
  // ok: pmx-js-sql-injection
  await db.query(`SELECT * FROM users`);
  // ok: pmx-js-sql-injection
  const label = "Hello " + id;
  // ruleid: pmx-js-prisma-raw-unsafe
  await prisma.$queryRawUnsafe(`SELECT * FROM users WHERE id = ${id}`);
  // ok: pmx-js-prisma-raw-unsafe
  await prisma.$queryRawUnsafe("SELECT 1");
  return label;
}

export function dom(el: HTMLElement, html: string, name: string) {
  // ruleid: pmx-js-dom-xss
  el.innerHTML = html;
  // ruleid: pmx-js-dom-xss
  el.insertAdjacentHTML("beforeend", "<b>" + name + "</b>");
  // ok: pmx-js-dom-xss
  el.innerHTML = "<br>";
  // ok: pmx-js-dom-xss
  el.innerHTML = DOMPurify.sanitize(html);
  // ok: pmx-js-dom-xss
  el.textContent = html;
}

export function tls() {
  // ruleid: pmx-js-tls-verification-disabled
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
  // ruleid: pmx-js-tls-verification-disabled
  const agent = new https.Agent({ keepAlive: true, rejectUnauthorized: false });
  // ok: pmx-js-tls-verification-disabled
  const ok = new https.Agent({ rejectUnauthorized: true });
  return [agent, ok];
}

export function redirect(req: any, res: any) {
  // ruleid: pmx-js-open-redirect
  res.redirect(req.query.next);
  // ok: pmx-js-open-redirect
  res.redirect("/home");
}

export function hashes(data: string) {
  // ruleid: pmx-js-weak-hash
  crypto.createHash("md5").update(data);
  // ok: pmx-js-weak-hash
  crypto.createHash("sha256").update(data);
}

export function jwts(jwt: any, token: string, key: string) {
  // ruleid: pmx-js-jwt-none-algorithm
  jwt.verify(token, key, { algorithms: ["HS256", "none"] });
  // ok: pmx-js-jwt-none-algorithm
  jwt.verify(token, key, { algorithms: ["RS256"] });
}

declare const run: (n: number) => void;
declare const DOMPurify: { sanitize(s: string): string };
declare const https: any;
