mod api;
mod files;
mod lsp;

use api::{Client, Result};
use std::{env, fs, path::{Path, PathBuf}, process::ExitCode};

/// Write problems.md in `dir`. Returns a short summary.
pub fn refresh(dir: &Path) -> Result<String> {
    let c = Client::new();
    let (user, problems) = c.problems()?;
    let daily = c.daily_slug().ok().and_then(|s| problems.iter().find(|p| p.slug == s));
    fs::write(dir.join(files::LIST_FILE), files::render_problem_list(&user, &problems, daily))
        .map_err(|e| e.to_string())?;
    Ok(format!("{} problems{}", problems.len(), if user.is_empty() { " (not signed in)".into() } else { format!(", signed in as {user}") }))
}

/// Solution file for `slug` in `dir`, created from the LeetCode template if missing.
pub fn open(dir: &Path, slug: &str, lang: &str) -> Result<PathBuf> {
    let q = Client::new().question(slug)?;
    if let Some(p) = files::find_solution(dir, &q.id, &q.slug) {
        return Ok(p);
    }
    let path = dir.join(files::solution_filename(&q.id, &q.slug, lang).ok_or(format!("unknown language {lang}"))?);
    fs::write(&path, files::render_solution(&q, lang)?).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Test (with the file's testcases) or submit a solution. Returns (passed, summary, details).
pub fn judge(text: &str, file_name: &str, submit: bool) -> Result<(bool, String, String)> {
    let s = files::parse_solution(text, file_name).ok_or("not a LeetCode solution file (missing `@lc app=leetcode` header)")?;
    let c = Client::new();
    let tests = match (&s.tests, submit) {
        (_, true) => None,
        (Some(t), _) => Some(t.clone()),
        (None, _) => Some(c.question(&s.slug)?.examples),
    };
    let r = c.judge(&s.slug, &s.lang, &s.code, tests.as_deref())?;
    Ok(api::format_result(&r, submit))
}

const TASKS: &str = r#"[
  { "label": "LeetCode: Test", "command": "leetcode-zed test \"$ZED_FILE\"", "reveal": "always" },
  { "label": "LeetCode: Submit", "command": "leetcode-zed submit \"$ZED_FILE\"", "reveal": "always" },
  { "label": "LeetCode: Open daily", "command": "zed \"$(leetcode-zed daily)\"", "hide": "on_success" },
  { "label": "LeetCode: Refresh problem list", "command": "leetcode-zed init", "hide": "on_success" }
]
"#;

const USAGE: &str = "leetcode-zed: LeetCode for Zed

  init                 write problems.md (and .zed/tasks.json if absent) in the current dir
  login                read your LeetCode cookie from stdin and save it
  whoami               show the signed-in user
  list [words...]      search problems
  pick <slug|id>       create the solution file in the current dir, print its path
  daily                same as pick, for today's daily problem
  test <file>          run the file against its testcases
  submit <file>        submit the file
  lsp                  run the language server (used by the Zed extension)

  --lang <lang>        language for pick/daily (default $LEETCODE_LANG or python3)";

fn main() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut lang = env::var("LEETCODE_LANG").unwrap_or_else(|_| "python3".into());
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        args.remove(i);
        if i < args.len() {
            lang = args.remove(i);
        }
    }
    let cwd = env::current_dir().unwrap_or_default();
    let arg = args.get(1).cloned().unwrap_or_default();
    let res: Result<String> = match args.first().map(String::as_str) {
        Some("lsp") => return lsp::run().map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        Some("init") => refresh(&cwd).map(|s| {
            let tasks = cwd.join(".zed/tasks.json");
            if !tasks.exists() && fs::create_dir_all(cwd.join(".zed")).is_ok() {
                let _ = fs::write(tasks, TASKS);
            }
            format!("wrote {} ({s})", files::LIST_FILE)
        }),
        Some("login") => {
            let mut cookie = String::new();
            eprintln!("Paste your leetcode.com cookie header, then Enter:");
            let _ = std::io::stdin().read_line(&mut cookie);
            let p = api::cookie_path();
            fs::create_dir_all(p.parent().unwrap())
                .and_then(|_| fs::write(&p, format!("{}{}\n", api::COOKIE_HELP, cookie.trim())))
                .map_err(|e| e.to_string())
                .and_then(|_| restrict(&p))
                .and_then(|_| Client::new().problems())
                .map(|(u, _)| if u.is_empty() { "saved, but LeetCode doesn't recognize it (expired or incomplete?)".into() } else { format!("signed in as {u}") })
        }
        Some("whoami") => Client::new().problems().map(|(u, _)| if u.is_empty() { "not signed in".into() } else { u }),
        Some("list") => Client::new().problems().map(|(_, ps)| {
            let words: Vec<String> = args[1..].iter().map(|w| w.to_lowercase()).collect();
            ps.iter()
                .filter(|p| {
                    let hay = format!("{} {} {}", p.id, p.title, p.difficulty).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .map(|p| format!("{:>5}  {:<6}  {}{}  ({})", p.id, p.difficulty, p.title, if p.status.as_deref() == Some("ac") { " ✅" } else { "" }, p.slug))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        Some("pick") if !arg.is_empty() => slug_for(&arg).and_then(|s| open(&cwd, &s, &lang)).map(|p| p.display().to_string()),
        Some("daily") => Client::new().daily_slug().and_then(|s| open(&cwd, &s, &lang)).map(|p| p.display().to_string()),
        Some(cmd @ ("test" | "submit")) if !arg.is_empty() => {
            let path = PathBuf::from(&arg);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            fs::read_to_string(&path).map_err(|e| format!("{arg}: {e}")).and_then(|t| judge(&t, &name, cmd == "submit")).and_then(
                |(ok, sum, details)| {
                    let out = format!("{sum}\n{details}");
                    if ok { Ok(out) } else { Err(out) }
                },
            )
        }
        _ => Err(USAGE.into()),
    };
    match res {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Accept a slug or a frontend id.
fn slug_for(arg: &str) -> Result<String> {
    if !arg.chars().all(|c| c.is_ascii_digit()) {
        return Ok(arg.to_string());
    }
    let (_, ps) = Client::new().problems()?;
    ps.into_iter().find(|p| p.id == arg).map(|p| p.slug).ok_or(format!("no problem #{arg}"))
}

/// The cookie is a credential: owner-only permissions.
pub fn restrict(p: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
