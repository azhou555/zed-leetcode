# Markdown "page" feature

Make a per-problem Markdown page that renders in Zed's preview and resembles the
LeetCode page as far as the theme allows (no custom CSS possible). Live-updates
with Test/Submit results; can also load solutions.

## Confirmed feasible (checked against Zed source + LeetCode API)
- Zed markdown preview renders remote images (assets.leetcode.com) and reparses on change.
- Preview has a pinned "Default" mode (`markdown: open preview to the side`) that stays on one
  file while you work in the code file — so results can stream into it.
- Community solutions: `ugcArticleSolutionArticles` (list) + `ugcArticleSolutionArticle(topicId)`
  returns Markdown `content`. Editorial: `question.solution{paidOnly canSeeDetail}` — free for many.
- html -> markdown via `htmd` crate (keeps images, tables, code).

## Design
- `open()` writes both the code file AND `<id>.<slug>.md` (the page).
- Page layout (with rewrite markers so Test/Submit touch only the result block):
  ```
  # [1] Two Sum   🟢 Easy
  [Open on LeetCode](url)   `two-sum`
  <!-- lc:result:start -->  (initially: "Run ▶ Test…")  <!-- lc:result:end -->
  ---
  <description: html->md, with images/tables/examples/constraints>
  <!-- lc:solutions:start --> (empty until loaded) <!-- lc:solutions:end -->
  ```
- Code file lenses gain **📄 Problem page** (opens the .md) and **💡 Solutions** (loads into page).
- Test/Submit: after judging, rewrite the page's result block with the verdict (same text as the
  popup, formatted as markdown), find the sibling page by `<id>.<slug>.md`. Keep the diagnostic too.
- Update mechanism: rewrite the file on disk; Zed reloads the unmodified buffer -> preview refreshes.
  If that proves flaky in-app, switch to `workspace/applyEdit`. (Can't verify preview headlessly.)
- `leetcode.page` and `leetcode.solutions` commands; advertise them.

## Steps
- [x] add `htmd`; `page.rs`: render_page(question), update_result(page, md), load_solutions
- [x] api.rs: `solutions(slug, n)` + `solution_body(topicId)` + editorial
- [x] open(): write page beside code
- [x] lsp: page/solutions commands + lenses; judge() updates page result block
- [x] unit tests: page render has markers+image, result-block replace is idempotent, solution md
- [x] smoke test: open writes .md; page/solutions commands; test updates result block
- [x] README + manual verify in Zed (preview refresh is the one thing to confirm live)

## Review
- 11 unit tests pass (page markers+image, set_block idempotent/scoped, find_solution ignores .md,
  solutions md); clippy clean; LSP smoke test passes incl. page creation + live solutions block rewrite
- live CLI: page generated for two-sum (markers, description) and add-two-numbers (image converted to
  ![](assets.leetcode.com/...)); both code + page files created; fixed find_solution picking up the .md
- NOT verified (needs live Zed + the preview open): that rewriting the .md on disk refreshes an open
  Markdown preview, and the real judge->page result update. If preview doesn't auto-refresh, switch the
  page writes to workspace/applyEdit.
