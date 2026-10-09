"""Exercise the production PowerShell HTTP adapter against loopback fixtures only."""
import argparse
import base64
import gzip
import json
import os
from pathlib import Path
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


TEXT = "你好，这是中文回答。\nLine 2: café 🙂"
PAYLOAD = json.dumps({"choices": [{"message": {"content": TEXT}}]}, ensure_ascii=False).encode("utf-8")
requests = []


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        requests.append((self.path, self.headers.get("Authorization"), self.headers.get("Content-Type"), body))
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/target")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        status = 401 if self.path == "/error" else 200
        data = b'{"error":"fixture error"}' if status == 401 else PAYLOAD
        if self.path == "/invalid":
            data = b'{"choices":\xff}'
        elif self.path == "/oversize":
            data = b" " * 1048577
        elif self.path == "/gzip":
            data = gzip.compress(data)
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=ISO-8859-1" if self.path == "/latin1" else "application/json")
        if self.path == "/gzip":
            self.send_header("Content-Encoding", "gzip")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The bounded adapter can reject oversized bodies before reading.


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--powershell", default="powershell")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = str(root / "src-tauri/scripts/ai.ps1").replace("'", "''")
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    uri = f"http://127.0.0.1:{server.server_port}"
    # -EncodedCommand avoids .ps1 file/argument encoding differences in PS 5.1.
    code = f"""
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$source=[IO.File]::ReadAllText('{source}',[Text.Encoding]::UTF8)
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$errors)
if($errors.Count){{throw 'Adapter syntax error'}}
foreach($f in $ast.FindAll({{param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]}},$false)){{Invoke-Expression $f.Extent.Text}}
$prompt=-join [char[]]@(0x4F60,0x597D)
$expected=(-join [char[]]@(0x4F60,0x597D,0xFF0C,0x8FD9,0x662F,0x4E2D,0x6587,0x56DE,0x7B54,0x3002))+"`nLine 2: caf"+[char]0xE9+' '+[char]::ConvertFromUtf32(0x1F642)
$body=New-SatoriAiRequestBody -Model 'fixture' -Prompt $prompt
foreach($path in @('/plain','/latin1','/gzip')){{
 $text=Invoke-SatoriAiRequest -Uri ('{uri}'+$path) -Body $body -Key 'fixture-key'
 if($text -cne $expected){{throw ('UTF-8 text changed: '+$path)}}
}}
foreach($path in @('/redirect','/invalid','/oversize','/error')){{
 $rejected=$false
 try{{$null=Invoke-SatoriAiRequest -Uri ('{uri}'+$path) -Body $body -Key 'fixture-key'}}catch{{$rejected=$true}}
 if(!$rejected){{throw ('Invalid response accepted: '+$path)}}
}}
[Console]::Write('AI HTTP: 7 fixture cases passed.')
"""
    encoded = base64.b64encode(code.encode("utf-16-le")).decode("ascii")
    env = dict(os.environ)
    env["NO_PROXY"] = env.get("NO_PROXY", "") + ",127.0.0.1,localhost"
    try:
        result = subprocess.run([args.powershell, "-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded], capture_output=True, timeout=60, env=env)
        if result.returncode:
            raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
        assert len(requests) == 7 and not any(r[0] == "/target" for r in requests), "Redirect unexpectedly followed"
        for _, authorization, content_type, body in requests:
            assert authorization == "Bearer fixture-key"
            assert "charset=utf-8" in content_type.lower()
            envelope = json.loads(body.decode("utf-8"))
            assert len(envelope["messages"]) == 2
            assert envelope["messages"][0]["role"] == "system"
            system = envelope["messages"][0]["content"]
            assert "Satori" in system and "没有执行权" in system and "敏感边界" in system
            assert "fixture-key" not in system and "tools" not in envelope
            assert envelope["messages"][1] == {"role": "user", "content": "你好"}
        print(result.stdout.decode("utf-8"))
        print("Verified UTF-8 request bytes, missing/misleading charset, gzip, blocked redirects, HTTP errors, invalid UTF-8 and body limit. No provider requests.")
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


if __name__ == "__main__":
    main()
