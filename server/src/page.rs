//! The per-problem Markdown "page": description (resembling the LeetCode page as
//! far as the Zed theme allows) plus blocks that Test/Submit/Solutions rewrite in
//! place, so an open Markdown preview updates live.
use crate::api::{BASE, Question};

pub fn page_filename(id: &str, slug: &str) -> String {
    format!("{id}.{slug}.md")
}

fn difficulty_badge(d: &str) -> &'static str {
    match d {
        "Easy" => "🟢 Easy",
        "Medium" => "🟡 Medium",
        "Hard" => "🔴 Hard",
        _ => "⚪ Unknown",
    }
}

/// `<!-- lc:NAME:start -->` ... `<!-- lc:NAME:end -->`
fn markers(name: &str) -> (String, String) {
    (format!("<!-- lc:{name}:start -->"), format!("<!-- lc:{name}:end -->"))
}

fn block(name: &str, inner: &str) -> String {
    let (a, b) = markers(name);
    format!("{a}\n{inner}\n{b}")
}

/// Replace the text between a block's markers, leaving the rest untouched.
/// Returns the input unchanged if the markers aren't both present.
pub fn set_block(page: &str, name: &str, inner: &str) -> String {
    let (a, b) = markers(name);
    let (Some(start), Some(end)) = (page.find(&a), page.find(&b)) else {
        return page.to_string();
    };
    if start > end {
        return page.to_string();
    }
    format!("{}{}", &page[..start], block(name, inner)) + &page[end + b.len()..]
}

/// Fix up LeetCode's HTML before htmd:
/// - htmd turns `<code>` *inside* a `<pre>` into a fenced code block, which shreds
///   the example blocks (`<pre>` wrapping `<strong>`/`<code>`/text) — make those inline.
/// - `<sup>5</sup>` would flatten to `105`; turn it into `^5` so `10^5` stays meaningful.
fn clean_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<pre") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find("</pre>") else { break };
        let block = &rest[..end + "</pre>".len()];
        // ponytail: LeetCode's in-example <code> has no attributes; plain-tag strip is enough.
        out.push_str(&block.replace("<code>", "`").replace("</code>", "`"));
        rest = &rest[end + "</pre>".len()..];
    }
    out.push_str(rest);
    out.replace("<sup>", "^").replace("</sup>", "")
}

pub fn render(q: &Question) -> String {
    let desc = match &q.content {
        Some(html) => htmd::convert(&clean_html(html)).unwrap_or_else(|_| html.clone()),
        None => "_Premium problem: description unavailable._".into(),
    };
    format!(
        "# [{id}] {title}\n\n{badge} · [Open on LeetCode]({base}/problems/{slug}/) · `{slug}`\n\n{result}\n\n---\n\n{desc}\n\n{solutions}\n",
        id = q.id,
        title = q.title,
        badge = difficulty_badge(&q.difficulty),
        base = BASE,
        slug = q.slug,
        result = block("result", "_Run ▶ Test or ⬆ Submit from the solution file; results show here._"),
        desc = desc.trim(),
        solutions = block("solutions", ""),
    )
}

/// Markdown for a Test/Submit verdict (summary line + detail lines from `format_result`).
pub fn result_markdown(summary: &str, details: &str, submit: bool, when: &str) -> String {
    let kind = if submit { "Submission" } else { "Test" };
    let body = if details.trim().is_empty() {
        String::new()
    } else {
        format!("\n```text\n{}\n```", details.trim_end())
    };
    format!("### {summary}\n_{kind} · {when}_{body}")
}

/// Markdown for the Solutions block: optional editorial + community articles.
pub fn solutions_markdown(editorial: Option<&str>, articles: &[(String, String)]) -> String {
    let mut s = String::from("## 💡 Solutions\n");
    if let Some(ed) = editorial {
        s += "\n### Official editorial\n\n";
        s += ed.trim();
        s += "\n";
    }
    for (title, body) in articles {
        s += &format!("\n<details>\n<summary>{}</summary>\n\n{}\n\n</details>\n", title.trim(), body.trim());
    }
    if editorial.is_none() && articles.is_empty() {
        s += "\n_No solutions available._\n";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q() -> Question {
        Question {
            question_id: "1".into(),
            id: "1".into(),
            title: "Two Sum".into(),
            slug: "two-sum".into(),
            difficulty: "Easy".into(),
            content: Some(concat!(
                "<p>Given <code>nums</code> where <code>n <= 10<sup>5</sup></code>.</p>",
                "<img src=\"https://assets.leetcode.com/x.png\" />",
                "<pre><strong>Input:</strong> s = \"abc\"\nNote that <code>&quot;bca&quot;</code> works.</pre>",
            ).into()),
            snippets: vec![],
            examples: "[2,7]\n9".into(),
        }
    }

    #[test]
    fn page_has_markers_and_image() {
        let p = render(&q());
        assert!(p.starts_with("# [1] Two Sum"));
        assert!(p.contains("🟢 Easy") && p.contains("https://leetcode.com/problems/two-sum/"));
        assert!(p.contains("![](https://assets.leetcode.com/x.png)"), "image not converted: {p}");
        for m in ["result", "solutions"] {
            assert!(p.contains(&format!("<!-- lc:{m}:start -->")) && p.contains(&format!("<!-- lc:{m}:end -->")));
        }
    }

    #[test]
    fn example_pre_blocks_stay_prose() {
        let p = render(&q());
        // the <pre> example must not become a fenced code block, and <strong>/<code> keep working
        assert!(!p.contains("```"), "example turned into a code fence: {p}");
        assert!(p.contains("**Input:**") && p.contains("`\"bca\"`"), "{p}");
        // <sup> becomes ^ so 10^5 isn't flattened to 105
        assert!(p.contains("10^5"), "superscript lost: {p}");
    }

    #[test]
    fn set_block_replaces_only_its_block() {
        let p = render(&q());
        let r = result_markdown("❌ Wrong Answer", "Case 2: ✗ output [1,1], expected [1,2]", false, "now");
        let p2 = set_block(&p, "result", &r);
        assert!(p2.contains("❌ Wrong Answer") && p2.contains("Case 2: ✗"));
        assert!(p2.contains("Given") && p2.contains("## 💡 Solutions") == false); // desc kept, solutions still empty
        // idempotent: replacing again doesn't nest or duplicate markers
        let p3 = set_block(&p2, "result", &result_markdown("✅ Accepted", "", true, "later"));
        assert_eq!(p3.matches("<!-- lc:result:start -->").count(), 1);
        assert!(p3.contains("✅ Accepted") && !p3.contains("Wrong Answer"));
        // unknown marker: unchanged
        assert_eq!(set_block(&p, "nope", "x"), p);
    }

    #[test]
    fn solutions_render() {
        let md = solutions_markdown(Some("# Approach\nUse a hashmap."), &[("Fast C++".into(), "```cpp\nint x;\n```".into())]);
        assert!(md.contains("Official editorial") && md.contains("hashmap"));
        assert!(md.contains("<summary>Fast C++</summary>") && md.contains("int x;"));
        assert!(solutions_markdown(None, &[]).contains("No solutions available"));
    }
}
