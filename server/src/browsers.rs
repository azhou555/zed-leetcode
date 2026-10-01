//! Import the LeetCode session cookie straight from a logged-in browser, so the
//! user doesn't have to dig it out of DevTools. Uses the `rookie` crate, which
//! reads each browser's cookie store (and on Chrome decrypts via the OS keychain).
use crate::api::Result;

/// The two cookie values the LeetCode API needs.
const WANTED: [&str; 2] = ["LEETCODE_SESSION", "csrftoken"];

type Reader = fn(Option<Vec<String>>) -> rookie::Result<Vec<rookie::enums::Cookie>>;

/// Browsers we try, in order. Firefox first: it needs no keychain prompt.
const BROWSERS: &[(&str, Reader)] = &[("Firefox", rookie::firefox), ("Chrome", rookie::chrome)];

/// Find a LeetCode session in a logged-in browser and return it as a `Cookie:` header value.
/// `only` restricts to one browser name (case-insensitive).
pub fn import(only: Option<&str>) -> Result<(String, String)> {
    if std::env::var_os("LEETCODE_NO_BROWSER").is_some() {
        return Err("browser import disabled ($LEETCODE_NO_BROWSER set)".into());
    }
    let mut tried = Vec::new();
    for (name, read) in BROWSERS {
        if only.is_some_and(|o| !o.eq_ignore_ascii_case(name)) {
            continue;
        }
        match read(Some(vec!["leetcode.com".into()])) {
            Ok(cookies) => {
                let header = to_header(&cookies);
                if header.contains("LEETCODE_SESSION=") {
                    return Ok((name.to_string(), header));
                }
                tried.push(format!("{name} (no LeetCode login found)"));
            }
            // a browser that isn't installed / has no profile is not an error, just skip it
            Err(e) => tried.push(format!("{name} ({e})")),
        }
    }
    Err(format!(
        "couldn't find a logged-in LeetCode session. Tried: {}.\nLog in at https://leetcode.com in Firefox or Chrome, then try again — or paste your cookie manually.",
        if tried.is_empty() { "no supported browser".into() } else { tried.join(", ") }
    ))
}

/// Keep only the cookies LeetCode needs, as `k=v; k=v`.
fn to_header(cookies: &[rookie::enums::Cookie]) -> String {
    cookies
        .iter()
        .filter(|c| WANTED.contains(&c.name.as_str()) && !c.value.is_empty())
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rookie::enums::Cookie;

    fn c(name: &str, value: &str) -> Cookie {
        Cookie {
            domain: "leetcode.com".into(),
            path: "/".into(),
            secure: true,
            expires: None,
            name: name.into(),
            value: value.into(),
            http_only: true,
            same_site: 0,
        }
    }

    #[test]
    fn filters_to_wanted() {
        let cookies = vec![c("LEETCODE_SESSION", "sess"), c("csrftoken", "tok"), c("ads_prefs", "junk"), c("csrftoken", "")];
        let h = to_header(&cookies);
        assert_eq!(h, "LEETCODE_SESSION=sess; csrftoken=tok");
        assert!(!h.contains("ads_prefs") && !h.contains("junk"));
    }

    #[test]
    fn no_session_is_not_a_match() {
        assert!(!to_header(&[c("csrftoken", "tok")]).contains("LEETCODE_SESSION="));
    }
}
