# zed-leetcode plan

Zed extensions can't add panels/webviews. What they *can* do: launch a language
server. Zed 1.22 renders LSP code lenses (clickable, `"code_lens": "on"`), code
actions (cmd-.), `workspace/symbol` (cmd-T), `window/showDocument`,
`window/showMessage` (prompt toast), `$/progress` (status bar) and diagnostics.
So the "UI" is: a generated `problems.md` browsed via outline/cmd-T + lenses, and
solution files with Test/Submit lenses.

## Architecture
```
extension.toml, Cargo.toml, src/lib.rs   WASM extension: starts `leetcode-zed lsp`
                                         (binary from settings path or $PATH) on
                                         all LeetCode languages + Markdown
server/  (bin `leetcode-zed`)            native CLI + LSP, one core
  api.rs    LeetCode HTTP (list, question, daily, interpret/submit + poll, whoami)
  files.rs  solution file format (vscode-leetcode compatible @lc markers), problems.md
  lsp.rs    lenses/actions/symbols/executeCommand
  main.rs   CLI: init | login | list | pick | test | submit | daily | whoami | lsp
```

## Solution file format (compatible with vscode-leetcode)
```
# @lc app=leetcode id=1 lang=python3 slug=two-sum
# [1] Two Sum (Easy)  <description as comments>
# @lc tests=start   <- editable custom testcases, defaults to examples
# @lc tests=end
# @lc code=start
...only this region is submitted...
# @lc code=end
```

## Features
- [x] problems.md: one `## 1. Two Sum · Easy · ✅` heading per problem (outline/cmd-T searchable)
- [x] Lens on headings: Open (fetch, write solution file, showDocument)
- [x] Lens at top of problems.md: Refresh · Daily · Random · Sign in
- [x] workspace/symbol: fuzzy problem search from any file
- [x] Solution file lenses: ▶ Test · ⬆ Submit · 🌐 Open in browser
- [x] Results: progress in status bar, prompt with verdict, diagnostic on code=start line
- [x] Auth: cookie file (~/.config/leetcode-zed/cookie) opened from "Sign in" lens / `login` CLI / env
- [x] Settings via `lsp.leetcode.initialization_options` (language, site)
- [x] CLI + `init` writes .zed/tasks.json (Test/Submit in terminal via $ZED_FILE)
- [x] Non-LeetCode files/projects: server stays silent
- [ ] Stretch: leetcode.cn base url, topic tags (skipped)

## Verification
- unit tests: header/tests/code parsing, filename, problems.md render
- live: list + question fetch + pick against leetcode.com (no auth)
- LSP smoke test: scripted JSON-RPC client (initialize, codeLens, symbol, executeCommand open)
- wasm build: `cargo build --target wasm32-wasip2`
- test/submit need a real cookie: verify request shape, ask user to confirm live

## Review
- unit tests (5) pass; clippy clean for server and wasm extension
- live: `init` (4069 problems + daily), `pick 1`, `pick lru-cache --lang rust`, `list two sum` against leetcode.com
- scripts/lsp_smoke.py passes: bootstrap lens on empty problems.md -> refresh, 4074 lenses, heading code action ->
  open writes file + showDocument + $/progress, cmd-t resolves to existing solution, solution lenses on code=start,
  Test without cookie -> "not signed in" prompt, sign-in creates 0600 cookie template, unrelated project is silent
- Zed source checked: commands must be advertised (they are), showDocument/showMessage/progress/code lens supported,
  default language_servers keep "..." so the server attaches
- NOT verified: real judge round-trip (needs the user's cookie) and in-Zed rendering (isolated Zed instance
  refuses to start while the user's Zed runs)
