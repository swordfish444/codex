**DOs**

- **Keep Prettier Coverage For JS**: ensure `codex-cli` JavaScript stays formatted in CI.
```json
// package.json
{
  "scripts": {
    "format": "prettier --check *.json *.md .github/workflows/*.yml **/*.js",
    "format:fix": "prettier --write *.json *.md .github/workflows/*.yml **/*.js"
  },
  "devDependencies": {
    "prettier": "^3.5.3"
  }
}
```

- **Use A Safe Dynamic Import Helper**: return `null` on failure; don’t throw here.
```js
async function tryImport(moduleName) {
  try {
    return await import(moduleName);
  } catch {
    return null;
  }
}
```

- **Name Functions For What They Return**: `resolveRgDir()` returns a directory, not a file path. Also, avoid double catches—check for `null`.
```js
import path from "node:path";

async function resolveRgDir() {
  const mod = await tryImport("@vscode/ripgrep");
  if (!mod?.rgPath) return null;
  return path.dirname(mod.rgPath);
}
```

- **Prepend To PATH With Correct Separator**: prefer `process.env.PATH` only; make new dirs take precedence.
```js
function getUpdatedPath(newDirs) {
  const sep = process.platform === "win32" ? ";" : ":";
  const existing = process.env.PATH || "";
  return [...newDirs, ...existing.split(sep).filter(Boolean)].join(sep);
}
```

- **Degrade Gracefully If ripgrep Missing**: only add the rg dir when available; still run the CLI.
```js
const extraDirs = [];
const rgDir = await resolveRgDir();
if (rgDir) extraDirs.push(rgDir);
const updatedPath = getUpdatedPath(extraDirs);
```

- **Pass Updated PATH Only To The Child**: don’t mutate global env; include `CODEX_MANAGED_BY_NPM`.
```js
const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env: { ...process.env, PATH: updatedPath, CODEX_MANAGED_BY_NPM: "1" }
});
```

- **Validate Against Node 20**: if module type is ambiguous, avoid top‑level `await`; wrap in an async entry point.
```js
(async function main() {
  const { spawn } = await import("node:child_process");
  // ...rest of startup logic...
})().catch((err) => {
  console.error("codex failed:", err);
  process.exit(1);
});
```

**DON’Ts**

- **Don’t Treat Env Vars As Case‑Sensitive On Windows**: skip `process.env.Path`.
```js
// ❌ Don’t
const existingPath = process.env.PATH || process.env.Path || "";
```

- **Don’t Throw When ripgrep Isn’t Installed**: it may be missing due to `--ignore-scripts` or network issues.
```js
// ❌ Don’t
if (!mod?.rgPath) {
  throw new Error("ripgrep not found");
}
```

- **Don’t Double‑Catch And Hide Logic Errors**: rely on `tryImport()` and test for `null`.
```js
// ❌ Don’t
async function resolveRgDir() {
  try {
    const { rgPath } = await import("@vscode/ripgrep");
    return path.dirname(rgPath);
  } catch (err) {
    console.error("unable to import ripgrep", err);
    return null;
  }
}
```

- **Don’t Append New Dirs To The End Of PATH**: rg should win precedence.
```js
// ❌ Don’t
const updatedPath = `${process.env.PATH}${sep}${newDirs.join(sep)}`;
```

- **Don’t Spam Errors For Optional Deps**: missing ripgrep isn’t fatal; keep logs minimal.
```js
// ❌ Don’t
console.error("rg missing; aborting startup");
// ✅ Prefer silent skip or a low‑noise debug log
```

- **Don’t Rely On Top‑Level Await In CJS**: it may break under the Node 20 minimum if the file isn’t ESM.
```js
// ❌ Don’t (in CommonJS)
const { spawn } = await import("node:child_process");
```