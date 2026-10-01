//! Language server: the "UI" Zed can host. Lenses and code actions on problems.md
//! headings and solution files, cmd-t problem search, results as prompts + diagnostics.
use crate::{api, files};
use crossbeam_channel::Sender;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicI32, Ordering},
};

const COMMANDS: &[&str] = &[
    "leetcode.open",
    "leetcode.test",
    "leetcode.submit",
    "leetcode.browser",
    "leetcode.refresh",
    "leetcode.daily",
    "leetcode.random",
    "leetcode.signin",
    "leetcode.page",
    "leetcode.solutions",
];

static NEXT_ID: AtomicI32 = AtomicI32::new(1);

struct State {
    root: Option<PathBuf>,
    lang: String,
    docs: HashMap<Url, String>,
}

pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (conn, io) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncKind::FULL.into()),
        code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
        code_action_provider: Some(true.into()),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: COMMANDS.iter().map(|c| c.to_string()).collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let init: InitializeParams = serde_json::from_value(conn.initialize(serde_json::to_value(caps)?)?)?;
    #[allow(deprecated)]
    let root = init
        .workspace_folders
        .and_then(|f| f.into_iter().next().map(|f| f.uri))
        .or(init.root_uri)
        .and_then(|u| u.to_file_path().ok());
    let lang = init
        .initialization_options
        .as_ref()
        .and_then(|o| o["language"].as_str().map(String::from))
        .or_else(|| std::env::var("LEETCODE_LANG").ok())
        .unwrap_or_else(|| "python3".into());
    let mut st = State { root, lang, docs: HashMap::new() };

    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    break;
                }
                handle_request(&conn.sender, &mut st, req);
            }
            Message::Notification(n) => handle_notification(&conn.sender, &mut st, n),
            Message::Response(_) => {} // replies to our showDocument / progress requests
        }
    }
    drop(conn); // writer thread exits once every sender is gone
    io.join()?;
    Ok(())
}

fn handle_notification(out: &Sender<Message>, st: &mut State, n: Notification) {
    match n.method.as_str() {
        "textDocument/didOpen" => {
            if let Ok(p) = serde_json::from_value::<DidOpenTextDocumentParams>(n.params) {
                st.docs.insert(p.text_document.uri, p.text_document.text);
            }
        }
        "textDocument/didChange" => {
            if let Ok(mut p) = serde_json::from_value::<DidChangeTextDocumentParams>(n.params)
                && let Some(c) = p.content_changes.pop()
            {
                // stale verdicts would mislead once the code changes
                if st.docs.get(&p.text_document.uri).is_some_and(|t| t.contains("@lc app=leetcode")) {
                    publish(out, &p.text_document.uri, vec![]);
                }
                st.docs.insert(p.text_document.uri, c.text);
            }
        }
        "textDocument/didClose" => {
            if let Ok(p) = serde_json::from_value::<DidCloseTextDocumentParams>(n.params) {
                st.docs.remove(&p.text_document.uri);
            }
        }
        _ => {}
    }
}

fn file_name(uri: &Url) -> String {
    uri.path_segments().and_then(|mut s| s.next_back()).unwrap_or("").to_string()
}

fn is_list(uri: &Url, text: &str) -> bool {
    file_name(uri) == files::LIST_FILE && (text.trim().is_empty() || text.starts_with(files::LIST_MARKER))
}

/// (line, title, command, args) for a document; shared by lenses and code actions.
fn actions(st: &State, uri: &Url, text: &str, line: Option<u32>) -> Vec<(u32, String, &'static str, Vec<Value>)> {
    let u = json!(uri.as_str());
    let mut v = Vec::new();
    if is_list(uri, text) {
        if text.trim().is_empty() {
            v.push((0, "⟳ Load LeetCode problems".into(), "leetcode.refresh", vec![u.clone()]));
        } else {
            for (t, c) in [("↻ Refresh", "leetcode.refresh"), ("📅 Daily", "leetcode.daily"), ("🎲 Random", "leetcode.random")] {
                v.push((0, t.into(), c, vec![u.clone()]));
            }
        }
        v.push((0, "🔑 Sign in".into(), "leetcode.signin", vec![]));
        let entries = files::parse_problem_list(text);
        // code actions: the heading at or above the cursor; lenses: every heading
        let picked: Vec<_> = match line {
            Some(l) => entries.iter().rev().find(|e| e.line <= l).into_iter().collect(),
            None => entries.iter().collect(),
        };
        for e in picked {
            let title = if line.is_some() { format!("Open {}", e.heading) } else { format!("▶ Open ({})", st.lang) };
            v.push((e.line, title, "leetcode.open", vec![u.clone(), json!(e.slug)]));
        }
    } else if let Some(s) = files::parse_solution(text, &file_name(uri)) {
        for (t, c) in [
            ("▶ Test", "leetcode.test"),
            ("⬆ Submit", "leetcode.submit"),
            ("📄 Problem page", "leetcode.page"),
            ("💡 Solutions", "leetcode.solutions"),
            ("🌐 Open in browser", "leetcode.browser"),
        ] {
            v.push((s.lens_line, t.into(), c, vec![u.clone()]));
        }
    }
    v
}

fn handle_request(out: &Sender<Message>, st: &mut State, req: Request) {
    let id = req.id.clone();
    if req.method == "workspace/executeCommand" {
        let Ok(p) = serde_json::from_value::<ExecuteCommandParams>(req.params) else {
            let _ = out.send(Response::new_err(id, -32602, "bad params".into()).into());
            return;
        };
        let uri = p.arguments.first().and_then(|a| a.as_str()).and_then(|s| Url::parse(s).ok());
        let text = uri
            .as_ref()
            .and_then(|u| st.docs.get(u).cloned().or_else(|| fs::read_to_string(u.to_file_path().ok()?).ok()))
            .unwrap_or_default();
        let slug = p.arguments.get(1).and_then(|a| a.as_str()).map(String::from);
        let (out, lang, root) = (out.clone(), st.lang.clone(), st.root.clone());
        // network calls take seconds: run off the main loop, answer when done
        std::thread::spawn(move || {
            if let Err(e) = execute(&out, &p.command, uri, text, slug, &lang, root) {
                message(&out, MessageType::ERROR, &format!("LeetCode: {e}"));
            }
            let _ = out.send(Response::new_ok(id, Value::Null).into());
        });
        return;
    }
    let result = (|| -> Result<Value, serde_json::Error> {
        Ok(match req.method.as_str() {
            "textDocument/codeLens" => {
                let p: CodeLensParams = serde_json::from_value(req.params)?;
                let text = st.docs.get(&p.text_document.uri).cloned().unwrap_or_default();
                let lenses: Vec<CodeLens> = actions(st, &p.text_document.uri, &text, None)
                    .into_iter()
                    .map(|(line, title, cmd, args)| CodeLens {
                        range: Range::new(Position::new(line, 0), Position::new(line, 0)),
                        command: Some(Command::new(title, cmd.into(), Some(args))),
                        data: None,
                    })
                    .collect();
                json!(lenses)
            }
            "textDocument/codeAction" => {
                let p: CodeActionParams = serde_json::from_value(req.params)?;
                let text = st.docs.get(&p.text_document.uri).cloned().unwrap_or_default();
                let acts: Vec<CodeActionOrCommand> = actions(st, &p.text_document.uri, &text, Some(p.range.start.line))
                    .into_iter()
                    .map(|(_, title, cmd, args)| {
                        let title = format!("LeetCode: {}", title.trim_start_matches(|c: char| !c.is_alphanumeric()));
                        CodeActionOrCommand::Command(Command::new(title, cmd.into(), Some(args)))
                    })
                    .collect();
                json!(acts)
            }
            "workspace/symbol" => {
                let p: WorkspaceSymbolParams = serde_json::from_value(req.params)?;
                json!(symbols(st, &p.query))
            }
            _ => Value::Null,
        })
    })();
    let resp = match result {
        Ok(v) => Response::new_ok(id, v),
        Err(e) => Response::new_err(id, -32602, e.to_string()),
    };
    let _ = out.send(resp.into());
}

/// Problem search for cmd-t, only inside a LeetCode workspace (the server also runs in unrelated projects).
fn symbols(st: &State, query: &str) -> Vec<SymbolInformation> {
    let Some(root) = &st.root else { return vec![] };
    let list = root.join(files::LIST_FILE);
    let Ok(text) = fs::read_to_string(&list) else { return vec![] };
    if !text.starts_with(files::LIST_MARKER) {
        return vec![];
    }
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    files::parse_problem_list(&text)
        .into_iter()
        .filter(|e| {
            let h = e.heading.to_lowercase();
            words.iter().all(|w| h.contains(w.as_str()))
        })
        .take(300)
        .map(|e| {
            // jump straight to an existing solution, else to the heading (where the Open lens is)
            let location = match files::find_solution(root, &e.id, &e.slug) {
                Some(p) => Location::new(Url::from_file_path(p).unwrap(), Range::default()),
                None => {
                    let pos = Position::new(e.line, 3);
                    Location::new(Url::from_file_path(&list).unwrap(), Range::new(pos, pos))
                }
            };
            #[allow(deprecated)]
            SymbolInformation {
                name: e.heading,
                kind: SymbolKind::EVENT,
                tags: None,
                deprecated: None,
                location,
                container_name: Some("LeetCode".into()),
            }
        })
        .collect()
}

fn execute(
    out: &Sender<Message>,
    cmd: &str,
    uri: Option<Url>,
    text: String,
    slug: Option<String>,
    lang: &str,
    root: Option<PathBuf>,
) -> api::Result<()> {
    // problems.md commands act on the list's folder; fall back to the workspace root
    let dir = uri
        .as_ref()
        .and_then(|u| u.to_file_path().ok())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or(root)
        .ok_or("no workspace folder")?;
    let show = |path: &Path| show_document(out, Url::from_file_path(path).unwrap(), false);
    match cmd {
        "leetcode.refresh" => {
            let done = progress(out, "Loading problem list");
            let r = crate::refresh(&dir);
            done();
            message(out, MessageType::INFO, &format!("LeetCode: {}", r?));
        }
        "leetcode.open" => {
            let slug = slug.ok_or("missing problem")?;
            let done = progress(out, &format!("Opening {slug}"));
            let r = crate::open(&dir, &slug, lang);
            done();
            let (code, _page) = r?;
            show(&code);
        }
        "leetcode.daily" => {
            let done = progress(out, "Opening daily problem");
            let r = api::Client::new().daily_slug().and_then(|s| crate::open(&dir, &s, lang));
            done();
            let (code, _page) = r?;
            show(&code);
        }
        "leetcode.page" => {
            let uri = uri.ok_or("missing file")?;
            let s = files::parse_solution(&text, &file_name(&uri)).ok_or("not a solution file")?;
            let page = dir.join(crate::page::page_filename(&s.id, &s.slug));
            if !page.exists() {
                let done = progress(out, "Building problem page");
                let r = crate::open(&dir, &s.slug, lang);
                done();
                r?;
            }
            show(&page);
            message(out, MessageType::INFO, "LeetCode: run `markdown: open preview to the side` to view the page rendered — it updates as you Test/Submit.");
        }
        "leetcode.solutions" => {
            let uri = uri.ok_or("missing file")?;
            let s = files::parse_solution(&text, &file_name(&uri)).ok_or("not a solution file")?;
            let done = progress(out, "Loading solutions");
            let r = (|| {
                if !dir.join(crate::page::page_filename(&s.id, &s.slug)).exists() {
                    crate::open(&dir, &s.slug, lang)?;
                }
                crate::load_solutions(&dir, &s.id, &s.slug)
            })();
            done();
            show(&r?);
            message(out, MessageType::INFO, "LeetCode: solutions added to the problem page (open its preview to read them).");
        }
        "leetcode.random" => {
            let text = fs::read_to_string(dir.join(files::LIST_FILE)).unwrap_or(text);
            let pool: Vec<_> = files::parse_problem_list(&text)
                .into_iter()
                .filter(|e| !e.heading.contains('✅') && !e.heading.contains('🔒'))
                .collect();
            if pool.is_empty() {
                return Err("no unsolved free problems in problems.md; refresh it first".into());
            }
            // ponytail: clock nanos as the dice; good enough to pick a problem
            let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as usize;
            let e = &pool[n % pool.len()];
            let pos = Position::new(e.line, 3);
            show_document_at(out, Url::from_file_path(dir.join(files::LIST_FILE)).unwrap(), Range::new(pos, pos));
        }
        "leetcode.signin" => {
            let done = progress(out, "Importing LeetCode session from your browser");
            let imported = crate::sign_in_from_browser(None);
            done();
            match imported {
                Ok(msg) => {
                    message(out, MessageType::INFO, &format!("LeetCode: {msg}. ↻ Refresh problems.md to see your progress."));
                    let _ = crate::refresh(&dir); // reflect solved status right away
                    show(&dir.join(files::LIST_FILE));
                }
                Err(e) => {
                    // no browser session found: fall back to pasting a cookie into a file
                    let p = api::cookie_path();
                    if !p.exists() {
                        api::save_cookie("").ok();
                    }
                    show(&p);
                    message(out, MessageType::WARNING, &format!("LeetCode: {e}\nOr paste your cookie into this file, save, then ↻ Refresh."));
                }
            }
        }
        "leetcode.browser" => {
            let s = files::parse_solution(&text, &file_name(uri.as_ref().unwrap())).ok_or("not a solution file")?;
            show_document(out, Url::parse(&format!("{}/problems/{}/description/", api::BASE, s.slug)).unwrap(), true);
        }
        "leetcode.test" | "leetcode.submit" => {
            let uri = uri.ok_or("missing file")?;
            let submit = cmd == "leetcode.submit";
            let done = progress(out, if submit { "Submitting" } else { "Running tests" });
            let r = crate::judge(&text, &file_name(&uri), submit);
            done();
            let (ok, summary, details) = r?;
            if let Some(s) = files::parse_solution(&text, &file_name(&uri)) {
                let sev = if ok { DiagnosticSeverity::INFORMATION } else { DiagnosticSeverity::ERROR };
                let range = Range::new(Position::new(s.lens_line, 0), Position::new(s.lens_line, 200));
                let d = Diagnostic::new(range, Some(sev), None, Some("leetcode".into()), format!("{summary}\n{details}"), None, None);
                publish(out, &uri, vec![d]);
                // stream the verdict into the problem page's preview, if one exists
                crate::write_page_result(&dir.join(crate::page::page_filename(&s.id, &s.slug)), &summary, &details, submit);
            }
            message(out, if ok { MessageType::INFO } else { MessageType::ERROR }, &format!("{summary}\n{details}"));
        }
        _ => return Err(format!("unknown command {cmd}")),
    }
    Ok(())
}

fn send_request(out: &Sender<Message>, method: &str, params: Value) {
    let id = RequestId::from(format!("lc-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)));
    let _ = out.send(Request::new(id, method.into(), params).into());
}

fn notify(out: &Sender<Message>, method: &str, params: Value) {
    let _ = out.send(Notification::new(method.into(), params).into());
}

fn message(out: &Sender<Message>, typ: MessageType, text: &str) {
    notify(out, "window/showMessage", json!(ShowMessageParams { typ, message: text.into() }));
}

fn publish(out: &Sender<Message>, uri: &Url, diagnostics: Vec<Diagnostic>) {
    notify(out, "textDocument/publishDiagnostics", json!(PublishDiagnosticsParams::new(uri.clone(), diagnostics, None)));
}

fn show_document(out: &Sender<Message>, uri: Url, external: bool) {
    send_request(out, "window/showDocument", json!({ "uri": uri, "external": external, "takeFocus": true }));
}

fn show_document_at(out: &Sender<Message>, uri: Url, selection: Range) {
    send_request(out, "window/showDocument", json!({ "uri": uri, "takeFocus": true, "selection": selection }));
}

/// Status-bar spinner; returns the closure that ends it.
fn progress(out: &Sender<Message>, title: &str) -> impl FnOnce() + use<> {
    let token = format!("leetcode-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    send_request(out, "window/workDoneProgress/create", json!({ "token": token }));
    notify(out, "$/progress", json!({ "token": token, "value": { "kind": "begin", "title": title } }));
    let out = out.clone();
    move || notify(&out, "$/progress", json!({ "token": token, "value": { "kind": "end" } }))
}
