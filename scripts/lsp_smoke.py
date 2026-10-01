#!/usr/bin/env python3
"""End-to-end smoke test of `leetcode-zed lsp` over JSON-RPC (hits leetcode.com, no auth needed).
Usage: scripts/lsp_smoke.py [path/to/leetcode-zed]"""
import json, os, pathlib, subprocess, sys, tempfile

BIN = sys.argv[1] if len(sys.argv) > 1 else "server/target/debug/leetcode-zed"


class Server:
    def __init__(self, root, env):
        self.p = subprocess.Popen([BIN, "lsp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env)
        self.id, self.inbox = 0, []
        r = self.request("initialize", {"processId": None, "rootUri": root.as_uri(), "capabilities": {},
                                        "initializationOptions": {"language": "python3"}})
        self.caps = r["capabilities"]
        self.notify("initialized", {})

    def send(self, msg):
        body = json.dumps({"jsonrpc": "2.0", **msg}).encode()
        self.p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.p.stdin.flush()

    def read(self):
        n = 0
        while (line := self.p.stdout.readline().strip()):
            if line.lower().startswith(b"content-length"):
                n = int(line.split(b":")[1])
        return json.loads(self.p.stdout.read(n))

    def notify(self, method, params):
        self.send({"method": method, "params": params})

    def request(self, method, params):
        self.id += 1
        self.send({"id": self.id, "method": method, "params": params})
        while True:
            m = self.read()
            if m.get("id") == self.id and "method" not in m:
                return m.get("result")
            self.inbox.append(m)

    def open(self, path):
        self.notify("textDocument/didOpen", {"textDocument": {"uri": path.as_uri(), "languageId": "x", "version": 1,
                                                              "text": path.read_text()}})

    def close(self):
        self.request("shutdown", None)
        self.notify("exit", None)
        self.p.wait(5)


def lenses(s, path):
    return s.request("textDocument/codeLens", {"textDocument": {"uri": path.as_uri()}})


def main():
    tmp = pathlib.Path(tempfile.mkdtemp())
    ws, other = tmp / "ws", tmp / "other"
    ws.mkdir(), other.mkdir()
    env = {**os.environ, "XDG_CONFIG_HOME": str(tmp / "config")}  # never touch the real cookie
    env.pop("LEETCODE_COOKIE", None)
    (ws / "problems.md").write_text("")

    s = Server(ws, env)
    assert set(s.caps["executeCommandProvider"]["commands"]) >= {"leetcode.open", "leetcode.test", "leetcode.submit"}

    # empty problems.md bootstraps via a lens
    s.open(ws / "problems.md")
    assert [l["command"]["command"] for l in lenses(s, ws / "problems.md")] == ["leetcode.refresh", "leetcode.signin"]
    s.request("workspace/executeCommand", {"command": "leetcode.refresh", "arguments": [(ws / "problems.md").as_uri()]})
    md = (ws / "problems.md").read_text()
    assert md.startswith("<!-- leetcode-zed") and "## 1. Two Sum" in md
    s.notify("textDocument/didChange", {"textDocument": {"uri": (ws / "problems.md").as_uri(), "version": 2},
                                        "contentChanges": [{"text": md}]})
    ls = lenses(s, ws / "problems.md")
    opens = [l for l in ls if l["command"]["command"] == "leetcode.open"]
    assert len(opens) > 3000, len(opens)
    print(f"problems.md: {len(ls)} lenses")

    # code action on the Two Sum heading opens it
    line = md.splitlines().index(next(l for l in md.splitlines() if l.startswith("## 1. Two Sum")))
    acts = s.request("textDocument/codeAction", {"textDocument": {"uri": (ws / "problems.md").as_uri()},
                                                 "range": {"start": {"line": line + 1, "character": 0}, "end": {"line": line + 1, "character": 0}},
                                                 "context": {"diagnostics": []}})
    open_act = next(a for a in acts if a["command"] == "leetcode.open")
    assert open_act["arguments"][1] == "two-sum", open_act
    s.inbox.clear()
    s.request("workspace/executeCommand", open_act)
    sol = ws / "1.two-sum.py"
    assert sol.exists()
    shown = [m for m in s.inbox if m.get("method") == "window/showDocument"]
    assert shown and shown[0]["params"]["uri"] == sol.as_uri(), s.inbox
    assert any(m.get("method") == "$/progress" for m in s.inbox)

    # symbols: search, existing solution resolves to the file
    syms = s.request("workspace/symbol", {"query": "two sum"})
    assert syms and syms[0]["location"]["uri"] == sol.as_uri(), syms[:2]
    assert any(x["location"]["uri"].endswith("problems.md") for x in syms)

    # solution file lenses on the code=start line; Test without cookie reports sign-in
    s.open(sol)
    ls = lenses(s, sol)
    start = sol.read_text().splitlines().index("# @lc code=start")
    assert [l["command"]["command"] for l in ls] == ["leetcode.test", "leetcode.submit", "leetcode.browser"]
    assert all(l["range"]["start"]["line"] == start for l in ls)
    s.inbox.clear()
    s.request("workspace/executeCommand", ls[0]["command"])
    msgs = [m["params"]["message"] for m in s.inbox if m.get("method") == "window/showMessage"]
    assert any("not signed in" in m for m in msgs), s.inbox

    # sign in creates a private cookie template and opens it
    s.inbox.clear()
    s.request("workspace/executeCommand", {"command": "leetcode.signin", "arguments": []})
    cookie = tmp / "config/leetcode-zed/cookie.key"
    assert cookie.exists() and oct(cookie.stat().st_mode & 0o777) == "0o600"
    assert any(m.get("method") == "window/showDocument" for m in s.inbox)
    s.close()

    # unrelated project: silent
    (other / "main.py").write_text("print(1)\n")
    o = Server(other, env)
    o.open(other / "main.py")
    assert lenses(o, other / "main.py") == []
    assert o.request("workspace/symbol", {"query": "two"}) == []
    o.close()
    print("lsp smoke: OK")


main()
