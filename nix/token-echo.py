# A web service on loopback for the journal VM test: answers any GET with a little JSON, and writes
# down the Authorization each request came with, for the test to find its token there and only there.
import http.server
import json


class Echo(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        with open("/var/lib/token-echo/seen", "a") as f:
            f.write((self.headers.get("Authorization") or "-") + "\n")
        body = json.dumps({"state": "ok", "queue": {"length": 3}}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


http.server.HTTPServer(("127.0.0.1", 8080), Echo).serve_forever()
