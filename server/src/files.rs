//! On-disk formats: solution files (vscode-leetcode compatible `@lc` markers) and problems.md.
use crate::api::{BASE, Problem, Question};
use std::path::Path;

pub const LIST_FILE: &str = "problems.md";
pub const LIST_MARKER: &str = "<!-- leetcode-zed";

/// (LeetCode lang slug, file extension, line comment)
const LANGS: &[(&str, &str, &str)] = &[
    ("cpp", "cpp", "//"),
    ("java", "java", "//"),
    ("python3", "py", "#"),
    ("python", "py", "#"),
    ("c", "c", "//"),
    ("csharp", "cs", "//"),
    ("javascript", "js", "//"),
    ("typescript", "ts", "//"),
    ("php", "php", "//"),
    ("swift", "swift", "//"),
    ("kotlin", "kt", "//"),
    ("dart", "dart", "//"),
    ("golang", "go", "//"),
    ("ruby", "rb", "#"),
    ("scala", "scala", "//"),
    ("rust", "rs", "//"),
    ("racket", "rkt", ";"),
    ("erlang", "erl", "%"),
    ("elixir", "ex", "#"),
];

fn lang_info(lang: &str) -> Option<(&'static str, &'static str)> {
    LANGS.iter().find(|l| l.0 == lang).map(|l| (l.1, l.2))
}

pub fn lang_names() -> String {
    LANGS.iter().map(|l| l.0).collect::<Vec<_>>().join(", ")
}

pub fn solution_filename(id: &str, slug: &str, lang: &str) -> Option<String> {
    lang_info(lang).map(|(ext, _)| format!("{id}.{slug}.{ext}"))
}

/// Existing solution file for a problem in `dir`, any language.
pub fn find_solution(dir: &Path, id: &str, slug: &str) -> Option<std::path::PathBuf> {
    let prefix = format!("{id}.{slug}.");
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix))
    })
}

pub fn render_solution(q: &Question, lang: &str) -> Result<String, String> {
    let (_, c) = lang_info(lang).ok_or_else(|| format!("unknown language {lang:?}; use one of: {}", lang_names()))?;
    let code = q
        .snippets
        .iter()
        .find(|(l, _)| l == lang)
        .map(|(_, code)| code.as_str())
        .ok_or_else(|| format!("{} has no {lang} template (premium-only or unsupported language)", q.title))?;
    let desc = match &q.content {
        Some(html) => html2text::from_read(html.as_bytes(), 90).unwrap_or_else(|_| html.clone()),
        None => "(Premium problem: description unavailable.)".into(),
    };
    let mut lines = vec![
        format!("@lc app=leetcode id={} lang={lang} slug={}", q.id, q.slug),
        String::new(),
        format!("[{}] {} ({})", q.id, q.title, q.difficulty),
        format!("{BASE}/problems/{}/", q.slug),
        String::new(),
    ];
    for l in desc.trim_end().lines() {
        if !(l.trim().is_empty() && lines.last().is_some_and(|p: &String| p.is_empty())) {
            lines.push(l.trim_end().to_string());
        }
    }
    lines.push(String::new());
    lines.push("Testcases for ▶ Test: one argument per line, edit freely.".into());
    lines.push("@lc tests=start".into());
    lines.extend(q.examples.lines().map(String::from));
    lines.push("@lc tests=end".into());
    let mut out: String =
        lines.iter().map(|l| format!("{c} {l}").trim_end().to_string() + "\n").collect();
    out += &format!("\n{c} @lc code=start\n{}\n{c} @lc code=end\n", code.trim_end_matches(['\n', '\r']));
    Ok(out)
}

#[derive(Debug, PartialEq)]
pub struct Solution {
    pub id: String,
    pub slug: String,
    pub lang: String,
    pub code: String,
    pub tests: Option<String>,
    pub lens_line: u32, // the `@lc code=start` line, or the header line
}

/// Parse a solution file. Slug falls back to the `{id}.{slug}.{ext}` file name (vscode-leetcode headers lack it).
pub fn parse_solution(text: &str, file_name: &str) -> Option<Solution> {
    let lines: Vec<&str> = text.lines().collect();
    let header_idx = lines.iter().position(|l| l.contains("@lc app=leetcode"))?;
    let field = |k: &str| {
        lines[header_idx]
            .split_whitespace()
            .find_map(|t| t.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
            .map(String::from)
    };
    let lang = field("lang")?;
    let id = field("id")?;
    let slug = field("slug").or_else(|| file_name.split('.').nth(1).map(String::from))?;
    let find = |m: &str| lines.iter().position(|l| l.contains(m));
    let between = |a: Option<usize>, b: Option<usize>| match (a, b) {
        (Some(a), Some(b)) if a < b => Some(&lines[a + 1..b]),
        _ => None,
    };
    let (start, end) = (find("@lc code=start"), find("@lc code=end"));
    let code = between(start, end).map(|l| l.join("\n")).unwrap_or_else(|| text.to_string());
    let comment = lang_info(&lang).map(|i| i.1).unwrap_or("#");
    let tests = between(find("@lc tests=start"), find("@lc tests=end"))
        .map(|ls| {
            ls.iter()
                .map(|l| {
                    let l = l.trim_start();
                    let l = l.strip_prefix(comment).unwrap_or(l);
                    l.strip_prefix(' ').unwrap_or(l).trim_end()
                })
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|t| !t.is_empty());
    Some(Solution { id, slug, lang, code, tests, lens_line: start.unwrap_or(header_idx) as u32 })
}

pub fn render_problem_list(user: &str, problems: &[Problem], daily: Option<&Problem>) -> String {
    let solved = problems.iter().filter(|p| p.status.as_deref() == Some("ac")).count();
    let mut s = format!(
        "{LIST_MARKER}: generated list, regenerate with the Refresh lens or `leetcode-zed init` -->\n# LeetCode\n\n"
    );
    s += &if user.is_empty() {
        "Not signed in: use the **Sign in** lens (or code action) above to paste your cookie.\n".to_string()
    } else {
        format!("Signed in as **{user}** · solved {solved}/{}\n", problems.len())
    };
    s += "\nSearch: `cmd-shift-o` (outline) or `cmd-t` (symbols). Open a problem with its lens or `cmd-.` on its heading.\n";
    let entry = |p: &Problem, prefix: &str| {
        let mark = match p.status.as_deref() {
            Some("ac") => " · ✅",
            Some("notac") => " · 🟡",
            _ => "",
        };
        let lock = if p.paid { " · 🔒" } else { "" };
        format!("\n## {prefix}{}. {} · {}{mark}{lock}\n`{}` · {:.1}% acceptance\n", p.id, p.title, p.difficulty, p.slug, p.ac_rate)
    };
    if let Some(d) = daily {
        s += &entry(d, "📅 Daily: ");
    }
    for p in problems {
        s += &entry(p, "");
    }
    s
}

pub struct Entry {
    pub line: u32,
    pub heading: String,
    pub id: String,
    pub slug: String,
}

/// Headings followed by a `` `slug` `` line.
pub fn parse_problem_list(text: &str) -> Vec<Entry> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .windows(2)
        .enumerate()
        .filter_map(|(i, w)| {
            let heading = w[0].strip_prefix("## ")?;
            let slug = w[1].strip_prefix('`')?.split('`').next()?;
            let id = heading.trim_start_matches("📅 Daily: ").split('.').next()?.to_string();
            Some(Entry { line: i as u32, heading: heading.to_string(), id, slug: slug.to_string() })
        })
        .collect()
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
            content: Some("<p>Given an array <code>nums</code>.</p><p>&nbsp;</p><p>&nbsp;</p><p>Return.</p>".into()),
            snippets: vec![("python3".into(), "class Solution:\n    def twoSum(self):\n        ".into())],
            examples: "[2,7,11,15]\n9".into(),
        }
    }

    #[test]
    fn solution_roundtrip() {
        let text = render_solution(&q(), "python3").unwrap();
        assert!(text.starts_with("# @lc app=leetcode id=1 lang=python3 slug=two-sum\n"), "{text}");
        assert!(text.contains("nums") && !text.contains("#\n#\n#\n"), "{text}");
        let s = parse_solution(&text, "1.two-sum.py").unwrap();
        assert_eq!((s.id.as_str(), s.slug.as_str(), s.lang.as_str()), ("1", "two-sum", "python3"));
        assert_eq!(s.code, "class Solution:\n    def twoSum(self):\n        ");
        assert_eq!(s.tests.as_deref(), Some("[2,7,11,15]\n9"));
        assert_eq!(text.lines().nth(s.lens_line as usize), Some("# @lc code=start"));
        assert!(render_solution(&q(), "rust").is_err());
        assert!(render_solution(&q(), "cobol").is_err());
    }

    #[test]
    fn vscode_leetcode_file() {
        let text = "/*\n * @lc app=leetcode id=1 lang=cpp\n *\n * [1] Two Sum\n */\n\n// @lc code=start\nclass Solution {};\n// @lc code=end\n";
        let s = parse_solution(text, "1.two-sum.cpp").unwrap();
        assert_eq!((s.slug.as_str(), s.code.as_str(), s.tests), ("two-sum", "class Solution {};", None));
        assert_eq!(s.lens_line, 6);
        assert!(parse_solution("fn main() {}", "main.rs").is_none());
    }

    #[test]
    fn problem_list_roundtrip() {
        let p = |id: &str, slug: &str, st: Option<&str>| Problem {
            id: id.into(),
            title: slug.into(),
            slug: slug.into(),
            difficulty: "Easy".into(),
            paid: id == "2",
            status: st.map(String::from),
            ac_rate: 50.0,
        };
        let ps = vec![p("1", "two-sum", Some("ac")), p("2", "add-two", None)];
        let md = render_problem_list("me", &ps, Some(&ps[1]));
        assert!(md.starts_with(LIST_MARKER) && md.contains("solved 1/2"));
        let es = parse_problem_list(&md);
        let got: Vec<_> = es.iter().map(|e| (e.id.as_str(), e.slug.as_str())).collect();
        assert_eq!(got, [("2", "add-two"), ("1", "two-sum"), ("2", "add-two")]);
        assert_eq!(es[1].heading, "1. two-sum · Easy · ✅");
        assert!(es[0].heading.ends_with("🔒"));
        assert_eq!(md.lines().nth(es[1].line as usize), Some("## 1. two-sum · Easy · ✅"));
    }
}
