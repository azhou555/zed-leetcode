use serde_json::{Value, json};
use std::{env, fs, path::PathBuf, thread, time::Duration};

pub const BASE: &str = "https://leetcode.com";
// ponytail: leetcode.cn has different GraphQL/daily shapes; add a `site` option when someone needs it.
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub id: String, // frontend id, what users see
    pub title: String,
    pub slug: String,
    pub difficulty: String,
    pub paid: bool,
    pub status: Option<String>, // "ac" | "notac"
    pub ac_rate: f64,
}

pub struct Question {
    pub question_id: String, // internal id, required by run/submit
    pub id: String,
    pub title: String,
    pub slug: String,
    pub difficulty: String,
    pub content: Option<String>,
    pub snippets: Vec<(String, String)>,
    pub examples: String,
}

pub fn cookie_path() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("leetcode-zed").join("cookie")
}

pub const COOKIE_HELP: &str = "\
# Paste your leetcode.com Cookie header below this comment block and save.
# How: log in at https://leetcode.com in your browser, open DevTools > Network,
# reload, click any request to leetcode.com, and copy the full `cookie` request
# header value. It must contain LEETCODE_SESSION=... and csrftoken=...
# Lines starting with # are ignored. Alternatively set $LEETCODE_COOKIE.
";

/// Cookie from $LEETCODE_COOKIE or the cookie file (comment lines ignored).
fn load_cookie() -> Option<String> {
    let raw = env::var("LEETCODE_COOKIE").ok().or_else(|| fs::read_to_string(cookie_path()).ok())?;
    let c = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("; ");
    let c = c.strip_prefix("cookie:").or(c.strip_prefix("Cookie:")).unwrap_or(&c).trim().to_string();
    (!c.is_empty()).then_some(c)
}

fn cookie_value(cookie: &str, key: &str) -> Option<String> {
    cookie
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_string())
}

pub struct Client {
    agent: ureq::Agent,
    cookie: Option<String>,
}

impl Client {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new().user_agent(UA).timeout(Duration::from_secs(30)).build();
        Client { agent, cookie: load_cookie() }
    }

    fn req(&self, method: &str, path: &str, referer: &str) -> ureq::Request {
        let mut r = self
            .agent
            .request(method, &format!("{BASE}{path}"))
            .set("Referer", referer)
            .set("Origin", BASE);
        if let Some(c) = &self.cookie {
            r = r.set("Cookie", c);
            if let Some(t) = cookie_value(c, "csrftoken") {
                r = r.set("x-csrftoken", &t);
            }
        }
        r
    }

    fn send(&self, r: ureq::Request, body: Option<Value>) -> Result<Value> {
        let res = match body {
            Some(b) => r.send_json(b),
            None => r.call(),
        };
        match res {
            Ok(resp) => resp.into_json().map_err(|e| format!("bad JSON from LeetCode: {e}")),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let hint = match code {
                    401 | 403 => " (not signed in or cookie expired?)",
                    429 => " (rate limited, wait a moment)",
                    _ => "",
                };
                Err(format!("LeetCode HTTP {code}{hint}: {}", body.chars().take(300).collect::<String>()))
            }
            Err(e) => Err(format!("network error: {e}")),
        }
    }

    fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let v = self.send(
            self.req("POST", "/graphql", BASE),
            Some(json!({ "query": query, "variables": variables })),
        )?;
        if let Some(e) = v.get("errors") {
            return Err(format!("GraphQL error: {e}"));
        }
        Ok(v["data"].clone())
    }

    /// All problems with the signed-in user's status, plus the username ("" when anonymous).
    pub fn problems(&self) -> Result<(String, Vec<Problem>)> {
        let v = self.send(self.req("GET", "/api/problems/all/", BASE), None)?;
        let mut out: Vec<Problem> = v["stat_status_pairs"]
            .as_array()
            .ok_or("unexpected problem list response")?
            .iter()
            .map(|p| {
                let s = &p["stat"];
                let sub = s["total_submitted"].as_f64().unwrap_or(0.0);
                Problem {
                    id: s["frontend_question_id"].to_string().trim_matches('"').to_string(),
                    title: s["question__title"].as_str().unwrap_or("").to_string(),
                    slug: s["question__title_slug"].as_str().unwrap_or("").to_string(),
                    difficulty: match p["difficulty"]["level"].as_i64() {
                        Some(1) => "Easy",
                        Some(2) => "Medium",
                        _ => "Hard",
                    }
                    .to_string(),
                    paid: p["paid_only"].as_bool().unwrap_or(false),
                    status: p["status"].as_str().map(String::from),
                    ac_rate: if sub > 0.0 { s["total_acs"].as_f64().unwrap_or(0.0) / sub * 100.0 } else { 0.0 },
                }
            })
            .collect();
        out.sort_by_key(|p| p.id.parse::<u32>().unwrap_or(u32::MAX));
        Ok((v["user_name"].as_str().unwrap_or("").to_string(), out))
    }

    pub fn question(&self, slug: &str) -> Result<Question> {
        let d = self.graphql(
            "query($s:String!){question(titleSlug:$s){questionId questionFrontendId title titleSlug difficulty content exampleTestcases codeSnippets{langSlug code}}}",
            json!({ "s": slug }),
        )?;
        let q = &d["question"];
        if q.is_null() {
            return Err(format!("no such problem: {slug}"));
        }
        let s = |k: &str| q[k].as_str().unwrap_or("").to_string();
        Ok(Question {
            question_id: s("questionId"),
            id: s("questionFrontendId"),
            title: s("title"),
            slug: s("titleSlug"),
            difficulty: s("difficulty"),
            content: q["content"].as_str().map(String::from),
            snippets: q["codeSnippets"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|c| (c["langSlug"].as_str().unwrap_or("").into(), c["code"].as_str().unwrap_or("").into()))
                        .collect()
                })
                .unwrap_or_default(),
            examples: s("exampleTestcases"),
        })
    }

    pub fn daily_slug(&self) -> Result<String> {
        let d = self.graphql("query{activeDailyCodingChallengeQuestion{question{titleSlug}}}", json!({}))?;
        d["activeDailyCodingChallengeQuestion"]["question"]["titleSlug"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| "no daily question found".into())
    }

    /// Run against `input` (Test) or submit when `input` is None. Blocks until judged.
    pub fn judge(&self, slug: &str, lang: &str, code: &str, input: Option<&str>) -> Result<Value> {
        if self.cookie.is_none() {
            return Err(format!("not signed in: put your LeetCode cookie in {}", cookie_path().display()));
        }
        let question_id = self.question(slug)?.question_id;
        let referer = format!("{BASE}/problems/{slug}/");
        let mut body = json!({ "lang": lang, "question_id": question_id, "typed_code": code });
        let (path, key) = match input {
            Some(i) => {
                body["data_input"] = i.into();
                body["judge_type"] = "large".into();
                (format!("/problems/{slug}/interpret_solution/"), "interpret_id")
            }
            None => (format!("/problems/{slug}/submit/"), "submission_id"),
        };
        let v = self.send(self.req("POST", &path, &referer), Some(body))?;
        let id = match &v[key] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(format!("LeetCode refused the request: {v}")),
        };
        for _ in 0..60 {
            thread::sleep(Duration::from_millis(1000));
            let r = self.send(self.req("GET", &format!("/submissions/detail/{id}/check/"), &referer), None)?;
            if r["state"] == "SUCCESS" {
                return Ok(r);
            }
        }
        Err("timed out waiting for the judge".into())
    }
}

/// (passed, one-line summary, multi-line details) from a judge `check` response.
pub fn format_result(r: &Value, submit: bool) -> (bool, String, String) {
    let s = |k: &str| match &r[k] {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    };
    let list = |k: &str| -> Vec<String> {
        r[k].as_array().map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()
    };
    let status = s("status_msg");
    let mut d = Vec::new();
    for k in ["full_compile_error", "full_runtime_error"] {
        if !s(k).is_empty() {
            d.push(s(k));
        }
    }
    let ok;
    if submit {
        ok = status == "Accepted";
        if ok {
            d.push(format!(
                "Runtime {} (beats {:.1}%), Memory {} (beats {:.1}%)",
                s("status_runtime"),
                r["runtime_percentile"].as_f64().unwrap_or(0.0),
                s("status_memory"),
                r["memory_percentile"].as_f64().unwrap_or(0.0)
            ));
        }
        if !s("total_testcases").is_empty() {
            d.push(format!("{}/{} testcases passed", s("total_correct"), s("total_testcases")));
        }
        for (label, k) in [("Input", "last_testcase"), ("Output", "code_output"), ("Expected", "expected_output"), ("Stdout", "std_output")] {
            if !s(k).is_empty() {
                d.push(format!("{label}: {}", s(k).replace('\n', ", ")));
            }
        }
    } else {
        let correct = r["correct_answer"].as_bool().unwrap_or(false);
        ok = r["run_success"].as_bool().unwrap_or(false) && correct;
        let (got, want, out) = (list("code_answer"), list("expected_code_answer"), list("std_output_list"));
        for (i, g) in got.iter().enumerate().filter(|(_, g)| !g.is_empty()) {
            let w = want.get(i).map(String::as_str).unwrap_or("");
            d.push(format!("Case {}: {} output {g}, expected {w}", i + 1, if g == w { "✓" } else { "✗" }));
            if let Some(o) = out.get(i).filter(|o| !o.is_empty()) {
                d.push(format!("  stdout: {}", o.trim_end().replace('\n', " | ")));
            }
        }
        if !s("status_runtime").is_empty() {
            d.push(format!("Runtime {}", s("status_runtime")));
        }
    }
    let summary = match status.as_str() {
        _ if ok && submit => "✅ Accepted".to_string(),
        _ if ok => "✅ All test cases passed".to_string(),
        "Accepted" => "❌ Wrong Answer".to_string(), // interpret ran fine but answers differ
        "" => format!("❌ Unexpected result: {r}"),
        _ => format!("❌ {status}"),
    };
    (ok, summary, d.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parsing() {
        let c = "LEETCODE_SESSION=abc; csrftoken=xyz; other=1";
        assert_eq!(cookie_value(c, "csrftoken").as_deref(), Some("xyz"));
        assert_eq!(cookie_value(c, "nope"), None);
    }

    #[test]
    fn results() {
        let run = json!({"state":"SUCCESS","status_msg":"Accepted","run_success":true,"correct_answer":false,
            "code_answer":["[0,1]","[2,1]"],"expected_code_answer":["[0,1]","[1,2]"],"std_output_list":["",""]});
        let (ok, sum, d) = format_result(&run, false);
        assert!(!ok);
        assert_eq!(sum, "❌ Wrong Answer");
        assert!(d.contains("Case 2: ✗ output [2,1], expected [1,2]"), "{d}");

        let sub = json!({"status_msg":"Accepted","status_runtime":"3 ms","runtime_percentile":91.5,
            "status_memory":"17 MB","memory_percentile":40.0,"total_correct":63,"total_testcases":63});
        let (ok, sum, d) = format_result(&sub, true);
        assert!(ok && sum == "✅ Accepted" && d.contains("beats 91.5%") && d.contains("63/63"), "{d}");

        let ce = json!({"status_msg":"Compile Error","full_compile_error":"line 3: oops"});
        let (ok, sum, d) = format_result(&ce, false);
        assert!(!ok && sum == "❌ Compile Error" && d.contains("oops"));
    }
}
