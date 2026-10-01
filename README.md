# LeetCode for Zed

Browse, solve, test and submit LeetCode problems without leaving Zed, in the
spirit of [vscode-leetcode](https://github.com/LeetCode-OpenSource/vscode-leetcode).

Zed extensions can't draw panels, so the UI is built from what Zed does render
from a language server:

| Where | What you get |
|---|---|
| `problems.md` | every problem as a heading (`## 1. Two Sum · Easy · ✅`), daily problem on top. Search with **cmd-shift-o** (outline). Each heading has a **▶ Open** lens; top line has **↻ Refresh · 📅 Daily · 🎲 Random · 🔑 Sign in** |
| anywhere in the workspace | **cmd-t** searches problems; picks an existing solution file or the heading in `problems.md` |
| solution files | **▶ Test · ⬆ Submit · 📄 Problem page · 💡 Solutions · 🌐 Open in browser** above `@lc code=start` |
| results | status-bar progress, a popup with the verdict / failing case, a diagnostic on the code block, **and a live section in the problem page** |
| problem page (`<id>.<slug>.md`) | the description with images/examples/constraints, rendered in Zed's Markdown preview; the result section updates on each Test/Submit, and **💡 Solutions** adds the editorial (when free) + top community solutions |

Every lens is also a code action (**cmd-.**), so it all works with lenses off too.

### The problem page

Opening a problem writes two files: the solution (`1.two-sum.py`) and a Markdown
page (`1.two-sum.md`). Open the page and run **`markdown: open preview to the side`**
once — it renders like a cleaned-up version of the LeetCode page (styled by your Zed
theme; custom CSS isn't possible for extensions). When you **Test**/**Submit** from the
solution file, the page's result section rewrites itself, so the preview shows the
latest verdict without you switching files. **💡 Solutions** fills the page's solutions
section from LeetCode.

## Install

Once the extension is in Zed's registry, install it from the Extensions panel.
The companion binary (`leetcode-zed`, the CLI + language server) is **downloaded
automatically** from this repo's GitHub releases for your platform — no manual
step. Recommended settings (`zed: open settings`):
```jsonc
{
  "code_lens": "on",
  "lsp": { "leetcode": { "initialization_options": { "language": "python3" } } }
}
```
Languages: cpp, java, python3, python, c, csharp, javascript, typescript, php,
swift, kotlin, dart, golang, ruby, scala, rust, racket, erlang, elixir.

### From source (development)

1. `cargo install --path server` (puts `leetcode-zed` on your `$PATH`; the
   extension uses it directly instead of downloading).
2. In Zed: `zed: install dev extension` → pick this repo folder.
3. The settings above. If the binary isn't on `$PATH`, point to it explicitly:
   `"lsp": { "leetcode": { "binary": { "path": "/abs/path/to/leetcode-zed", "arguments": ["lsp"] } } }`.

Binary resolution order: `lsp.leetcode.binary.path` → `leetcode-zed` on `$PATH`
→ GitHub release download.

## Publishing

- **Binaries:** push a tag (`git tag v0.1.0 && git push origin v0.1.0`); the
  `release` workflow cross-compiles and attaches the per-platform assets that
  `src/lib.rs` downloads.
- **Registry:** fork `zed-industries/extensions`, add this repo as a submodule
  and an entry in its `extensions.toml`, then open a PR.

## Use

1. Make an empty folder, create an empty `problems.md` in it, open it in Zed.
   Click **⟳ Load LeetCode problems** (or `leetcode-zed init` in a terminal).
2. **🔑 Sign in** imports your LeetCode session straight from **Firefox** or **Chrome**
   (just be logged into leetcode.com in one of them). Firefox needs no prompt; Chrome asks
   once for Keychain access to "Chrome Safe Storage" to decrypt its cookies. Only
   `LEETCODE_SESSION` and `csrftoken` are read, and they're saved to
   `~/.config/leetcode-zed/cookie.key` (readable only by you).
   - From a terminal: `leetcode-zed login` (or `login firefox` / `login chrome`).
   - No browser access? `leetcode-zed login paste` reads a cookie header from stdin, the
     **Sign in** lens falls back to opening `cookie.key` for you to paste into, and
     `$LEETCODE_COOKIE` is honored. Set `$LEETCODE_NO_BROWSER=1` to disable browser reading.
3. Open a problem → a `1.two-sum.py` file with the description, editable testcases
   between `@lc tests=start/end`, and the template between `@lc code=start/end`.
   Only the code region is sent. Files are compatible with vscode-leetcode.

## CLI and Zed tasks

```
leetcode-zed init | login [browser|paste] | whoami | list [words] | pick <slug|id> | daily | test <file> | submit <file>
```
`init` also writes `.zed/tasks.json` with **LeetCode: Test / Submit** tasks, so you
can get the judge output in the terminal (`task: spawn`), or bind them to keys:
```jsonc
{ "context": "Editor", "bindings": { "cmd-alt-t": ["task::Spawn", { "task_name": "LeetCode: Test" }] } }
```

## Develop

```sh
cd server && cargo test                 # unit tests
scripts/lsp_smoke.py                    # drives the LSP over JSON-RPC against leetcode.com
cargo build --target wasm32-wasip2      # the extension itself
```
Logs: `zed: open log`, look for `leetcode`.

Not done (yet): leetcode.cn, topic-tag filters, auto-downloading the binary
(needs published releases).
