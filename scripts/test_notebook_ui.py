#!/usr/bin/env python3
"""Real browser + app API + isolated PostgreSQL; external services are local mocks.

Run with `uv run --with playwright scripts/test_notebook_ui.py --help`.
Never uses an existing database, credentials or paid model/provider services.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import signal
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
TOKEN = "local-notebook-ui-session-fixture"
USER = "00000000-0000-4000-8000-000000000001"
ORG = "00000000-0000-4000-8000-000000000002"
PROJECT = "00000000-0000-4000-8000-000000000003"
GRAPH = "00000000-0000-4000-8000-000000000004"
GH = "00000000-0000-4000-8000-000000000005"
BASE_ARGS = {"slug": "notebook-fixture", "project": "sheets", "graph": "lifecycle"}
STATE = {"log": [], "vault": {}, "sessions": {}, "running": False, "session_n": 0}
BUILD = {}
def SQL(query) -> str:
    raise RuntimeError("fixture database has not started")


def manifest():
    cells = json.loads(SQL("select coalesce(json_agg(c order by position), '[]') from graph_cells c"))
    functions, program, pending, nodes, ids = [], [], [], [], {}
    for cell in cells:
        config, name = cell["config"], cell["name"]
        ty = config.get("output_type") or ("String" if cell["kind"] == "sink" else "Nat")
        if cell["kind"] == "source" and cell["variant"] == "ui":
            input_name = config.get("input", name)
            program.append(f'  let {name} ← input "{input_name}" {ty}')
            ids[name] = len(nodes)
            nodes.append({"id": len(nodes), "input": input_name})
        else:
            args = config.get("dependencies", [])
            signature = " → ".join(["Nat"] * len(args) + [f"Eff [] {ty}"])
            functions.append({"name": name, "module": "Sheet", "function": f"Sheet.{name}", "signature": signature})
            pending.append(cell)
    while pending:
        ready = next((c for c in pending if all(d in ids for d in c["config"].get("dependencies", []))), None)
        if ready is None:
            raise ValueError("fixture received a cyclic graph")
        pending.remove(ready)
        name = ready["name"]
        args = ready["config"].get("dependencies", [])
        program.append(f'  let {name} ← {name} ' + " ".join(args))
        ids[name] = len(nodes)
        nodes.append({"id": len(nodes), "function": name, "args": [ids[a] for a in args]})
    sinks = [n["id"] for n in nodes if not any(n["id"] in other.get("args", []) for other in nodes)]
    BUILD.update({"id": "B1", "state": "ready", "diagnostics": [], "functions": functions,
                  "graphs": [{"name": "main", "inputs": [n["input"] for n in nodes if "input" in n],
                              "nodes": nodes, "sources": [n["id"] for n in nodes if not n.get("args")], "sinks": sinks}]})
    return {"open": ["Sheet"], "functions": functions, "graphs": [{"name": "main", "program": "do\n" + "\n".join(program)}]}


def evaluate(inputs):
    out = []
    for original in BUILD["graphs"][0]["nodes"]:
        node = dict(original)
        if "input" in node:
            if node["input"] in inputs:
                node["output"] = inputs[node["input"]]
        else:
            deps = [out[i] for i in node.get("args", [])]
            missing = next((n for n in deps if "output" not in n), None)
            if missing:
                node["skipped"] = missing["id"]
            elif node["function"] == "total" and inputs.get("amount") == 99:
                node["error"] = "fixture computation refused amount 99"
            elif node["function"] == "view":
                node["output"] = f"Total: {deps[0]['output']}" if deps else "Empty"
            else:
                node["output"] = sum(n["output"] for n in deps) if deps else 7
        out.append(node)
    return out


class Mock(BaseHTTPRequestHandler):
    service = ""

    def log_message(self, format, *args):
        pass

    def reply(self, status, body):
        raw = body.encode() if isinstance(body, str) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "text/plain" if isinstance(body, str) else "application/json")
        self.send_header("content-length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def handle_request(self, method):
        raw = self.rfile.read(int(self.headers.get("content-length", 0)))
        body = json.loads(raw) if raw else {}
        path = urlparse(self.path).path
        STATE["log"].append([self.service, method, path, body])
        if self.service == "vault":
            if path.endswith("/login"):
                return self.reply(200, {"auth": {"client_token": "fixture", "lease_duration": 3600}})
            STATE["vault"][path] = body
            return self.reply(200, {})
        if self.service == "broker":
            call = body["call"]
            assert call["kind"] == "connector" and "url" not in call
            payload = json.loads(call["payload"])
            resource = call["resource"]
            if resource and resource[-1] == "lun.json":
                contents = json.dumps(manifest()).encode()
                answer = json.dumps({"encoding": "base64", "content": base64.b64encode(contents).decode()}).encode()
            elif resource and resource[-1] == "Sheet.lean":
                contents = b"namespace Sheet\ndef total (amount tax : Nat) : Eff [] Nat := pure (amount + tax)\nend Sheet\n"
                answer = json.dumps({"encoding": "base64", "content": base64.b64encode(contents).decode()}).encode()
            elif payload.get("view") == "branch":
                answer = json.dumps({"commit": {"sha": "c0ffee" * 6 + "abcd"}}).encode()
            else:
                answer = b'{}'
            return self.reply(200, {"status": 200, "body": answer.hex()})
        if self.service == "writer":
            if path == "/v0/sessions" and method == "POST":
                STATE["writer_tools"] = body["tools"]
                return self.reply(201, {"id": "L1", "state": "running"})
            if path.endswith("/messages"):
                if method == "POST":
                    assert set(body.get("tools", [])) <= set(STATE["writer_tools"])
                    STATE["writer_tools"] = body.get("tools", STATE["writer_tools"])
                    return self.reply(202, {"queued": 0})
                return self.reply(200, {"entries": [{"index": 0, "type": "assistant", "text": "Writing total from amount and tax"}], "next": 1, "running": STATE["running"]})
            if path.endswith("/diff"):
                return self.reply(200, "diff --git a/Sheet.lean b/Sheet.lean\n@@ -1 +1 @@\n-def total := 0\n+def total := 7\n")
            if path.endswith("/credentials"):
                return self.reply(200, {})
            return self.reply(200, {"id": "L1", "state": "running" if STATE["running"] else "idle", "entries": 1,
                                    "tools": STATE.get("writer_tools", []),
                                    "workspace": {"remoteHead": "c0ffee" * 6 + "abcd"}})
        if self.service == "runtime":
            if path.startswith("/v0/builds") and not path.endswith("/sessions"):
                manifest()
                return self.reply(200, BUILD)
            if path.endswith("/sessions"):
                STATE["session_n"] += 1
                sid = f"S{STATE['session_n']}"
                inputs = dict(body.get("inputs", {}))
                STATE["sessions"][sid] = {"inputs": inputs, "start": body}
                return self.reply(201, {"session": sid, "nodes": evaluate(inputs)})
            sid = path.rsplit("/", 1)[-1]
            if method == "DELETE":
                STATE["sessions"].pop(sid, None)
                return self.reply(200, {})
            if sid not in STATE["sessions"]:
                return self.reply(404, {"error": "fixture runtime forgot the session"})
            session = STATE["sessions"][sid]
            before = evaluate(session["inputs"])
            session["inputs"].update(body.get("inputs", {}))
            after = evaluate(session["inputs"])
            return self.reply(200, {"nodes": after, "changed": [a for a, b in zip(after, before) if a != b], "updates": 1})
        return self.reply(404, {})

    def do_GET(self): self.handle_request("GET")
    def do_POST(self): self.handle_request("POST")
    def do_PUT(self): self.handle_request("PUT")
    def do_DELETE(self): self.handle_request("DELETE")


def verify(page, context, app, output):
    from playwright.sync_api import expect
    results = []
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.set_default_timeout(20000)
    context.add_cookies([{"name": "tn_session", "value": TOKEN, "url": app}])
    context.route("**/*", lambda route: route.continue_() if urlparse(route.request.url).hostname in ("127.0.0.1", "localhost") else route.abort())
    notebook = app + "/orgs/notebook-fixture/projects/sheets/graphs/lifecycle"

    def passed(name):
        results.append(name)
        print("PASS", name, flush=True)

    def wait_sql(query, expected):
        deadline = time.monotonic() + 10
        actual = None
        while time.monotonic() < deadline:
            actual = SQL(query)
            if actual == expected:
                return
            time.sleep(.1)
        assert actual == expected, (actual, expected)

    def go(url):
        page.goto(url, wait_until="networkidle")
        expect(page.locator(".nb-editor, .settings-body").first).to_be_visible()

    def cell(name):
        return page.locator(".nb-cell").filter(has=page.locator(".nb-cell-name", has_text=name))

    def choose(label, option):
        # The dx Select applies aria-label to its wrapper, not its trigger.
        wrapper = page.locator(f'[aria-label="{label}"]')
        wrapper.get_by_role("button").click()
        page.get_by_role("option", name=option, exact=True).click()

    def add(name, kind="Node", dependencies="", ty="Nat", choices=""):
        editor = page.locator(".nb-editor").last
        choose("Kind of cell", kind)
        editor.get_by_label("Name", exact=True).fill(name)
        editor.get_by_label("Description", exact=True).fill("Fixture " + name)
        if kind == "Node" or kind == "Sink · notebook output":
            editor.get_by_label("Input cells (in argument order)").fill(dependencies)
        editor.get_by_label("Output type (optional Lean 4 constraint)").fill(ty)
        if choices:
            editor.get_by_label("Choices (optional, comma-separated)").fill(choices)
        editor.get_by_role("button", name="Add cell", exact=True).click()
        expect(cell(name)).to_be_visible()

    go(notebook)
    output.joinpath("initial-dom.html").write_text(page.content())
    add("tax", "Source · input in the notebook")
    add("amount", "Source · input in the notebook")
    add("total", dependencies="amount, tax")
    add("view", "Sink · notebook output", dependencies="total", ty="String")
    passed("create typed input, multi-argument computation and display cells")
    expect(page.locator(".nb-cell-name")).to_have_text(["tax", "amount", "total", "view"])
    choose("Cell order", "Name")
    expect(page.locator(".nb-cell-name")).to_have_text(["amount", "tax", "total", "view"])
    expect(page.locator(".nb-cell-index")).to_have_text(["2", "1", "3", "4"])
    expect(page.get_by_role("button", name="Move tax up")).to_have_count(0)
    choose("Cell order", "Dependency order")
    expect(page.locator(".nb-cell-name")).to_have_text(["amount", "tax", "total", "view"])
    choose("Cell order", "Declaration order")
    cell("amount").get_by_role("button", name="Move amount up").click()
    expect(page.locator(".nb-cell-name")).to_have_text(["amount", "tax", "total", "view"])
    passed("name/topological sort preserves declaration numbers; declaration movement persists")
    page.get_by_role("button", name="Dependency graph", exact=True).click()
    expect(page.locator(".nb-graph-edge")).to_have_count(3)
    page.get_by_role("button", name="Go to cell amount").focus()
    page.keyboard.press("Enter")
    expect(page.locator(".nb-cell-name")).to_have_text(["amount"])
    page.get_by_role("button", name="Go to cell total").click()
    expect(page.locator(".nb-cell-name")).to_have_text(["total"])
    page.evaluate("window.scrollTo(0, 0)")
    page.screenshot(path=str(output / "graph-desktop.png"), full_page=True)
    page.get_by_role("button", name="Notebook", exact=True).click()
    passed("dependency graph edges and cell navigation")
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    expect(edit.get_by_label("Description", exact=True)).to_have_value("Fixture total")
    edit.get_by_label("Input cells (in argument order)").fill("missing")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(edit.locator(".orgs-error")).to_contain_text("no cell named @missing")
    edit.get_by_label("Input cells (in argument order)").fill("view")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(edit.locator(".orgs-error")).to_contain_text("cycle")
    edit.get_by_label("Input cells (in argument order)").fill("total")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(edit.locator(".orgs-error")).to_contain_text("itself")
    edit.get_by_label("Input cells (in argument order)").fill("amount, tax")
    description = "Sum @amount and @tax; literal </textarea><b>text</b> & symbols"
    edit.get_by_label("Description", exact=True).fill(description)
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(cell("total").locator(".nb-description")).to_have_text(description)
    passed("edit description/dependencies; unknown, cyclic and self references rejected without losing form")
    go(notebook)
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    expect(edit.get_by_label("Description", exact=True)).to_have_value(description)
    expect(edit.locator("b")).to_have_count(0)
    edit.get_by_role("button", name="Cancel", exact=True).click()
    passed("saved descriptions survive SSR/hydration reload; HTML-like textarea text remains literal")
    for name, renamed in [("amount", "price"), ("price", "amount")]:
        cell(name).get_by_role("button", name="Edit", exact=True).click()
        edit = page.locator(".nb-cell.editing")
        edit.get_by_label("Name", exact=True).fill(renamed)
        edit.get_by_role("button", name="Save", exact=True).click()
        expect(cell(renamed)).to_be_visible()
        expect(cell("total").locator(".nb-description")).to_contain_text("@" + renamed)
        assert json.loads(SQL("select config from graph_cells where name='total'"))["dependencies"] == [renamed, "tax"]
    passed("renaming a referenced cell atomically updates named arguments and explicit prose references")
    cell("amount").get_by_role("button", name="Delete", exact=True).click()
    cell("amount").get_by_role("button", name="Confirm delete", exact=True).click()
    expect(cell("amount").locator(".orgs-error")).to_contain_text("references")
    add("scratch")
    cell("scratch").get_by_role("button", name="Delete", exact=True).click()
    cell("scratch").get_by_role("button", name="Confirm delete", exact=True).click()
    expect(cell("scratch")).to_have_count(0)
    passed("confirmed delete succeeds for unreferenced cells and refuses referenced cells")
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    edit.locator(".cell-connections > summary").click()
    edit.get_by_role("button", name="S3 bucket · Reports fixture", exact=True).click()
    permission = edit.locator(".cell-connection").filter(has_text="Reports fixture")
    permission.get_by_role("button", name="Read only", exact=True).click()
    permission.locator(".permission-advanced > summary").click()
    permission.get_by_label("objects.read resource", exact=True).fill("reports/2026")
    permission.get_by_role("button", name="Include descendants").last.click()
    permission.get_by_label("Maximum response bytes").fill("4096")
    assert SQL("select config->'connectors' is null from graph_cells where name='total'") == "t"
    edit.get_by_role("button", name="Save", exact=True).click()
    config = json.loads(SQL("select config from graph_cells where name='total'"))
    assert config["connectors"][0]["permissions"]["scopes"][1]["root"] == ["reports", "2026"]
    assert config["connectors"][0]["permissions"]["maxResponseBytes"] == 4096
    passed("cell connection picker persists narrowed structured resource grants and byte limits")
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    edit.locator(".cell-connections > summary").click()
    permission = edit.locator(".cell-connection").filter(has_text="Reports fixture")
    permission.locator(".permission-advanced > summary").click()
    expect(permission.get_by_role("button", name="Write objects", exact=True)).to_be_disabled()
    permission.get_by_label("Maximum response bytes").fill("16777217")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(edit.locator(".orgs-error").last).to_contain_text("cannot widen")
    edit.get_by_role("button", name="Cancel", exact=True).click()
    passed("cell cannot add denied operations or raise inherited byte limits; API rejects widened grants")
    add("base", dependencies="", ty="Nat")
    page.get_by_role("button", name="Generate all code", exact=True).click()
    expect(page.get_by_role("button", name="Restart session", exact=True)).to_be_visible(timeout=45000)
    expect(cell("amount").locator('input[type="number"]')).to_be_visible()
    expect(cell("base").locator(".nb-output")).to_contain_text("7")
    expect(cell("total").locator(".nb-impl")).to_contain_text("Nat → Nat → Eff [] Nat")
    cell("total").get_by_role("button", name="Show the code ▾", exact=True).click()
    expect(cell("total").locator(".nb-code")).to_contain_text("def total")
    cell("amount").locator('input[type="number"]').fill("10")
    cell("amount").get_by_role("button", name="Feed input", exact=True).click()
    cell("tax").locator('input[type="number"]').fill("2")
    cell("tax").get_by_role("button", name="Feed input", exact=True).click()
    expect(cell("view").locator(".nb-output")).to_contain_text("Total: 12")
    passed("generate all → adopted build → typed numeric widgets → feed → dependent output")
    STATE["sessions"].clear()
    cell("amount").locator('input[type="number"]').fill("20")
    cell("amount").get_by_role("button", name="Feed input", exact=True).click()
    expect(cell("view").locator(".nb-output")).to_contain_text("Total: 22")
    session = STATE["sessions"][f"S{STATE['session_n']}"]
    assert session["start"]["inputs"] == {"amount": 20, "tax": 2}
    assert session["start"]["binding"] == {"org_id": ORG, "user_id": USER, "graph_id": GRAPH}
    passed("runtime session loss re-registers with latest input log and tenant/user binding")
    cell("amount").locator('input[type="number"]').fill("99")
    cell("amount").get_by_role("button", name="Feed input", exact=True).click()
    expect(cell("total").locator(".nb-error")).to_contain_text("fixture computation refused")
    passed("runtime computation errors remain visible with recovery controls")
    STATE["running"] = True
    cell("total").get_by_role("button", name="Regenerate code", exact=True).click()
    expect(cell("total").locator(".nb-phase")).to_have_text("writing the code")
    assert SQL("select count(*) from graph_cells where writing") == "1"
    STATE["running"] = False
    expect(page.get_by_role("button", name="Restart session", exact=True)).to_be_visible(timeout=45000)
    assert any("Regenerate this cell" in json.dumps(entry) for entry in STATE["log"] if entry[0] == "writer")
    passed("regenerate one marks only its cell and follows writing/build lifecycle")
    STATE["running"] = True
    page.get_by_role("button", name="Regenerate all code", exact=True).click()
    expect(page.get_by_role("button", name="Abort", exact=True)).to_be_visible()
    assert SQL("select count(*) from graph_cells where writing") == "5"
    STATE["running"] = False
    expect(page.get_by_role("button", name="Restart session", exact=True)).to_be_visible(timeout=45000)
    passed("regenerate all marks every cell and returns to live session")
    cell("amount").locator('input[type="number"]').fill("20")
    cell("amount").get_by_role("button", name="Feed input", exact=True).click()
    expect(cell("view").locator(".nb-output")).to_contain_text("Total: 22")
    cell("amount").locator('input[type="number"]').fill("21")
    choose("Cell order", "Name")
    expect(cell("amount").locator('input[type="number"]')).to_have_value("21")
    choose("Cell order", "Declaration order")
    expect(cell("amount").locator('input[type="number"]')).to_have_value("21")
    passed("sorting preserves unsent source values and keyed cell state")
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    edit.get_by_label("Input cells (in argument order)").fill("amount")
    edit.get_by_label("Description", exact=True).fill("Use @amount only")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(cell("total").locator(".nb-phase")).to_have_text("code older than the description")
    expect(cell("total").locator(".nb-declarations")).not_to_contain_text("@tax")
    cell("total").get_by_role("button", name="Regenerate code", exact=True).click()
    expect(cell("total").locator(".nb-impl")).to_contain_text("Nat → Eff [] Nat", timeout=45000)
    expect(cell("view").locator(".nb-output")).to_contain_text("Total: 20")
    cell("amount").locator('input[type="number"]').fill("25")
    cell("amount").get_by_role("button", name="Feed input", exact=True).click()
    expect(cell("view").locator(".nb-output")).to_contain_text("Total: 25")
    passed("changing dependencies → stale draft → regenerate → new signature and reactive output")
    cell("tax").get_by_role("button", name="Edit", exact=True).click()
    page.locator(".nb-cell.editing").get_by_label("Output type (optional Lean 4 constraint)").fill("String")
    page.locator(".nb-cell.editing").get_by_label("Choices (optional, comma-separated)").fill("low, high")
    page.locator(".nb-cell.editing").get_by_role("button", name="Save", exact=True).click()
    expect(cell("tax").locator('input[type="number"]')).to_be_visible()
    page.get_by_role("button", name="Regenerate all code", exact=True).click()
    expect(cell("tax").locator('input[type="number"]')).to_have_count(0, timeout=45000)
    expect(cell("tax").locator(".nb-widget").get_by_role("button")).to_be_visible()
    choose("tax", "high")
    expect(cell("tax").locator(".orgs-error")).to_have_count(0)
    expect(page.get_by_role("button", name="Restart session", exact=True)).to_be_visible()
    wait_sql("select value #>> '{}' from graph_inputs where input='tax' order by at desc limit 1", "high")
    passed("source type edit keeps the running numeric widget; rebuilt String choices feed successfully")
    page.evaluate("window.scrollTo(0, 0)")
    page.screenshot(path=str(output / "notebook-desktop.png"), full_page=True)
    page.set_viewport_size({"width": 390, "height": 844})
    page.screenshot(path=str(output / "notebook-mobile.png"), full_page=True)
    assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    page.locator(".nb-cell.editing").get_by_label("Name", exact=True).focus()
    assert page.evaluate("document.activeElement.tagName") == "INPUT"
    page.locator(".nb-cell.editing").get_by_role("button", name="Cancel", exact=True).click()
    passed("390px layout has no page overflow; editable fields receive keyboard focus")
    page.set_viewport_size({"width": 1280, "height": 900})
    go(app + "/orgs/notebook-fixture/settings/connections")
    row = page.locator(".conn-row").filter(has=page.locator(".conn-label", has_text="Reports fixture"))
    row.locator(".connection-permissions > summary").click()
    row.get_by_role("button", name="Read and write", exact=True).click()
    row.locator(".permission-advanced > summary").click()
    row.get_by_label("objects.write resource", exact=True).fill("reports")
    row.get_by_role("button", name="Save connection permissions", exact=True).click()
    assert json.loads(SQL("select permissions from connections where provider='s3'"))["scopes"][2]["root"] == ["reports"]
    assert any("permissions" in key for key in STATE["vault"])
    passed("connection preset and resource ceiling saves through real API into DB and mock vault")
    for provider, label, boundary, required in [
        ("gdrive", "Drive fixture", "folder-123", "files.update"),
        ("dropbox", "Dropbox fixture", "notes/project", "files.create"),
        ("azure", "Azure fixture", "reports/2026", "objects.write"),
        ("google-calendar", "Calendar fixture", "primary", "events.create"),
        ("gmail", "Mail fixture", "INBOX", "drafts.create"),
        ("notion", "Notion fixture", "page-123", "pages.update"),
        ("slack", "Slack fixture", "C-fixture", "messages.update"),
        ("github", "Repository fixture", "fixture/sheets", "repositories.write"),
        ("anthropic", "Anthropic fixture", "fixture-model", "inference.generate"),
    ]:
        row = page.locator(".conn-row").filter(has=page.locator(".conn-label", has_text=label))
        row.locator(".connection-permissions > summary").click()
        before = SQL(f"select coalesce(permissions::text, 'null') from connections where provider='{provider}'")
        row.get_by_role("button", name="Read and write", exact=True).click()
        row.get_by_label("Resource boundary (optional)", exact=True).fill(boundary)
        assert SQL(f"select coalesce(permissions::text, 'null') from connections where provider='{provider}'") == before
        row.get_by_role("button", name="Save connection permissions", exact=True).click()
        wait_sql(f"select permissions->'scopes'->0->'root' from connections where provider='{provider}'", json.dumps(boundary.split('/')))
        saved = json.loads(SQL(f"select permissions from connections where provider='{provider}'"))
        assert all(scope["root"] == boundary.split('/') for scope in saved["scopes"])
        assert any(scope["operation"] == required for scope in saved["scopes"])
        assert not any(scope["operation"].endswith(("send", "delete", "share", "invite")) for scope in saved["scopes"])
    passed("quick scoped presets persist across storage, files, calendar, mail, workspace, messaging, repository and AI without opening Advanced")
    go(notebook)
    choose("Kind of cell", "Sink · storage")
    editor = page.locator(".nb-editor").last
    editor.get_by_label("Name", exact=True).fill("archive")
    editor.get_by_label("Description", exact=True).fill("Archive this report")
    editor.get_by_label("Path", exact=True).fill("reports/latest.json")
    choose("Storage connection", "S3 bucket · Reports fixture")
    editor.locator(".cell-connections > summary").click()
    editor.get_by_role("button", name="S3 bucket · Reports fixture", exact=True).click()
    editor.get_by_role("button", name="Use configured object: reports/latest.json", exact=True).click()
    editor.get_by_role("button", name="Add cell", exact=True).click()
    expect(cell("archive")).to_be_visible()
    scopes = json.loads(SQL("select config from graph_cells where name='archive'"))["connectors"][0]["permissions"]["scopes"]
    assert any(scope["operation"] == "objects.write" for scope in scopes)
    assert all(scope["root"] == ["reports", "latest.json"] and not scope["descendants"] for scope in scopes)
    cell("archive").get_by_role("button", name="Delete", exact=True).click()
    cell("archive").get_by_role("button", name="Confirm delete", exact=True).click()
    expect(cell("archive")).to_have_count(0)
    passed("storage path shortcut infers exact object authority under the saved bucket ceiling")
    go(app + "/orgs/notebook-fixture/settings/notebooks")
    page.locator('summary', has_text="Advanced connector ceilings").click()
    ceiling = page.locator(".provider-ceiling").filter(has=page.locator("summary", has_text="Google Calendar"))
    ceiling.locator("summary").first.click()
    ceiling.get_by_role("button", name="Read and write", exact=True).click()
    ceiling.locator(".permission-advanced > summary").click()
    ceiling.get_by_label("events.create resource", exact=True).fill("primary")
    page.get_by_role("button", name="Save permissions", exact=True).click()
    expect(page.get_by_text("Notebook permissions saved.", exact=True)).to_be_visible()
    policy = json.loads(SQL("select effect_policy from orgs"))
    grants = policy["connectorCeilings"]["google-calendar"]["scopes"]
    assert any(g["operation"] == "events.create" and g["root"] == ["primary"] for g in grants)
    assert not any(g["operation"] in ("events.delete", "events.invite") for g in grants)
    passed("organization provider ceiling uses non-destructive preset and saves structured calendar scope")
    before = SQL("select effect_policy::text from orgs")
    page.get_by_role("button", name="Domains: use each cell's configured URL", exact=True).click()
    page.get_by_label("Allowed HTTP domains (one per line)").fill("127.0.0.1")
    assert SQL("select effect_policy::text from orgs") == before
    page.get_by_role("button", name="Save permissions", exact=True).click()
    expect(page.locator(".orgs-error").last).to_contain_text("public DNS hostname")
    page.get_by_label("Allowed HTTP domains (one per line)").fill("api.example.com")
    page.get_by_role("button", name="Save permissions", exact=True).click()
    wait_sql("select effect_policy->>'configuredDomains' from orgs", "false")
    policy = json.loads(SQL("select effect_policy from orgs"))
    policy["connectorCeilings"]["s3"] = {"scopes": [{"operation": "objects.read", "root": ["reports"], "descendants": True}], "maxRequestBytes": 1048576, "maxResponseBytes": 2048}
    # The organization UI uses the same editor already exercised above; set
    # this narrower fixture via SQL to isolate the cell API/UI boundary test.
    SQL("update orgs set effect_policy='" + json.dumps(policy) + "'::jsonb")
    go(notebook)
    cell("total").get_by_role("button", name="Edit", exact=True).click()
    edit = page.locator(".nb-cell.editing")
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(edit.locator(".orgs-error").last).to_contain_text("organization's connector ceiling")
    edit.locator(".cell-connections > summary").click()
    permission = edit.locator(".cell-connection").filter(has_text="Reports fixture")
    permission.get_by_role("button", name="Read only", exact=True).click()
    edit.get_by_role("button", name="Save", exact=True).click()
    expect(cell("total")).to_be_visible()
    saved = json.loads(SQL("select config from graph_cells where name='total'"))["connectors"][0]["permissions"]
    assert saved["maxResponseBytes"] == 2048
    assert all(scope["operation"] == "objects.read" for scope in saved["scopes"])
    choose("Kind of cell", "Sink · HTTP POST")
    editor = page.locator(".nb-editor").last
    editor.get_by_label("Name", exact=True).fill("webhook")
    editor.get_by_label("Description", exact=True).fill("Post this report")
    editor.get_by_label("POST to", exact=True).fill("https://other.example.com/report")
    editor.get_by_role("button", name="Add cell", exact=True).click()
    expect(editor.locator(".orgs-error")).to_contain_text("organization has not allowed HTTP access")
    assert SQL("select count(*) from graph_cells where name='webhook'") == "0"
    passed("invalid domains retain the form; explicit HTTP allowlist and organization connector ceilings are enforced by cell API")
    go(app + "/orgs/notebook-fixture/settings/notebooks")
    expect(page.get_by_label("Allowed HTTP domains (one per line)")).to_have_value("api.example.com")
    page.locator('summary', has_text="Advanced connector ceilings").click()
    page.locator(".provider-ceiling").filter(has=page.locator("summary", has_text="Google Calendar")).locator("summary").first.click()
    page.evaluate("window.scrollTo(0, 0)")
    page.screenshot(path=str(output / "permissions-desktop.png"), full_page=True)
    assert not errors, errors
    passed("no browser JavaScript errors")
    output.joinpath("results.json").write_text(json.dumps({"passed": results, "page_errors": errors}, indent=2))


def main():
    global SQL
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app-port", type=int, default=18754)
    parser.add_argument("--pg-port", type=int, default=55459)
    parser.add_argument("--mock-port", type=int, default=58310)
    parser.add_argument("--pg-bin", type=Path, default=Path("/opt/homebrew/opt/postgresql@16/bin") if Path("/opt/homebrew/opt/postgresql@16/bin/postgres").exists() else None, help="Directory with a complete PostgreSQL server installation")
    parser.add_argument("--output-parent", type=Path, default=Path(tempfile.gettempdir()) / "opencode")
    parser.add_argument("--cleanup-fixture", type=Path, help="Remove only a stopped fixture's PostgreSQL data, preserving screenshots/logs")
    args = parser.parse_args()
    if args.cleanup_fixture:
        fixture = args.cleanup_fixture.resolve()
        if fixture.parent != args.output_parent.resolve() or not fixture.name.startswith("notebook-ui-"):
            parser.error("cleanup accepts only notebook-ui-* directories in output-parent")
        pg = fixture / "pg"
        if pg.joinpath("postmaster.pid").exists():
            parser.error("refusing to remove a potentially running PostgreSQL fixture")
        if pg.exists():
            shutil.rmtree(pg)
        return
    for port in [args.app_port, args.pg_port] + list(range(args.mock_port, args.mock_port + 4)):
        with socket.socket() as probe:
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            probe.bind(("127.0.0.1", port))
    output = Path(tempfile.mkdtemp(prefix="notebook-ui-", dir=args.output_parent))
    print("Artifacts:", output, flush=True)
    app = f"http://127.0.0.1:{args.app_port}"
    pgdata = output / "pg"
    proc, servers, pg_started = None, [], False
    log = output.joinpath("app.log").open("w")
    pg_tool = lambda name: str(args.pg_bin / name) if args.pg_bin else name
    try:
        subprocess.run([pg_tool("initdb"), "-D", str(pgdata), "-U", "postgres", "-A", "trust", "--no-locale"], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([pg_tool("pg_ctl"), "-D", str(pgdata), "-l", str(output / "postgres.log"), "-o", f"-h 127.0.0.1 -p {args.pg_port} -k ''", "-w", "start"], check=True, stdout=subprocess.DEVNULL)
        pg_started = True
        env = {**os.environ, "PGSSLMODE": "disable"}
        command = [pg_tool("psql"), "-X", "-h", "127.0.0.1", "-p", str(args.pg_port), "-U", "postgres", "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-qtAc"]
        def sql(query):
            return subprocess.run(command + [query], check=True, capture_output=True, text=True, env=env).stdout.strip()
        SQL = sql
        for migration in sorted(ROOT.joinpath("migrations").glob("*.sql")):
            sql(migration.read_text())
        sql(f"""
          insert into users(id,email,display_name) values('{USER}','notebook@example.invalid','Notebook UI fixture');
          insert into orgs(id,slug,name,auto_repairs) values('{ORG}','notebook-fixture','Notebook UI fixture',0);
          insert into memberships(user_id,org_id,role) values('{USER}','{ORG}','owner');
          insert into sessions(id_hash,user_id,expires_at) values(sha256(convert_to('{TOKEN}','UTF8')),'{USER}',now()+interval '1 day');
          insert into connections(id,org_id,user_id,provider,label,base_url,status,permissions) values('{GH}','{ORG}','{USER}','github','Repository fixture','https://api.github.com','active',
            '{{"scopes":[{{"operation":"repositories.read","root":[],"descendants":true}},{{"operation":"repositories.write","root":[],"descendants":true}}],"maxRequestBytes":1048576,"maxResponseBytes":16777216}}');
          insert into connections(org_id,user_id,provider,label,base_url,status) values
            ('{ORG}','{USER}','anthropic','Anthropic fixture','https://api.anthropic.com/v1','active'),
            ('{ORG}','{USER}','s3','Reports fixture','https://fixture.s3.amazonaws.com','active'),
             ('{ORG}','{USER}','gdrive','Drive fixture','https://www.googleapis.com','active'),
             ('{ORG}','{USER}','dropbox','Dropbox fixture','https://api.dropboxapi.com','active'),
             ('{ORG}','{USER}','azure','Azure fixture','https://fixture.blob.core.windows.net/reports','active'),
             ('{ORG}','{USER}','notion','Notion fixture','https://api.notion.com/v1','active'),
             ('{ORG}','{USER}','slack','Slack fixture','https://slack.com/api','active'),
            ('{ORG}','{USER}','google-calendar','Calendar fixture','https://www.googleapis.com','active'),
            ('{ORG}','{USER}','gmail','Mail fixture','https://gmail.googleapis.com','active');
          insert into projects(id,org_id,slug,name,created_by,repo_connection_id,repo_provider,repo_full_name,repo_web_url,repo_default_branch)
            values('{PROJECT}','{ORG}','sheets','Sheets','{USER}','{GH}','github','fixture/sheets','https://github.com/fixture/sheets','main');
          insert into graphs(id,project_id,slug,name,created_by,model_connection_id,model_name)
            values('{GRAPH}','{PROJECT}','lifecycle','Notebook lifecycle','{USER}',(select id from connections where provider='anthropic'),'fixture-model');
        """)
        for offset, service in enumerate(["vault", "broker", "writer", "runtime"]):
            server = ThreadingHTTPServer(("127.0.0.1", args.mock_port + offset), type(service, (Mock,), {"service": service}))
            servers.append(server)
            threading.Thread(target=server.serve_forever, daemon=True).start()
        env.update({"DATABASE_URL": f"postgres://postgres@127.0.0.1:{args.pg_port}/postgres?sslmode=disable", "PUBLIC_URL": app,
                    "SECRETS_URL": f"http://127.0.0.1:{args.mock_port}", "SECRETS_PASSWORD": "fixture",
                    "LIAISON_URL": f"http://127.0.0.1:{args.mock_port + 1}", "LIAISON_ROOT_KEY": "42" * 32,
                    "LODE_URL": f"http://127.0.0.1:{args.mock_port + 2}", "LODE_TOKEN": "fixture",
                    "LUN_URL": f"http://127.0.0.1:{args.mock_port + 3}", "LUN_TOKEN": "fixture", "TYPEDNOTES_AUTO_REPAIRS": "0"})
        # Real services cannot be reached through the fixture mocks. No compute DB is needed.
        env.pop("COMPUTE_DB_URL", None)
        proc = subprocess.Popen(["dx", "serve", "-p", "web", "--fullstack", "true", "--debug-symbols", "false", "--port", str(args.app_port),
                                 "--addr", "127.0.0.1", "--open", "false", "--interactive", "false", "--watch", "false", "--hot-reload", "false"],
                                cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + 300
        while time.monotonic() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f"dx exited: see {output / 'app.log'}")
            if "Build failed" in output.joinpath("app.log").read_text():
                raise RuntimeError(f"dx build failed: see {output / 'app.log'}")
            try:
                with urlopen(app + "/api/health", timeout=2) as response:
                    if response.status == 200:
                        break
            except Exception:
                time.sleep(1)
        else:
            raise TimeoutError("app did not start within 300 seconds")
        from playwright.sync_api import sync_playwright
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(headless=True)
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = context.new_page()
            try:
                verify(page, context, app, output)
            except Exception:
                page.screenshot(path=str(output / "failure.png"), full_page=True)
                output.joinpath("failure-dom.html").write_text(page.content())
                output.joinpath("failure-state.json").write_text(json.dumps({"build": BUILD, "sessions": STATE["sessions"], "graph": json.loads(SQL("select row_to_json(g) from graphs g"))}, indent=2))
                raise
            finally:
                browser.close()
    finally:
        output.joinpath("mock-requests.json").write_text(json.dumps(STATE["log"], indent=2))
        if proc and proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try: proc.wait(timeout=15)
            except subprocess.TimeoutExpired: os.killpg(proc.pid, signal.SIGKILL)
        log.close()
        for server in servers:
            server.shutdown()
            server.server_close()
        if pg_started:
            subprocess.run([pg_tool("pg_ctl"), "-D", str(pgdata), "-m", "immediate", "-w", "stop"], check=True, stdout=subprocess.DEVNULL)
            shutil.rmtree(pgdata)


if __name__ == "__main__":
    main()
