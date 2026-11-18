# A mock ntfy server on loopback for the events VM test: writes down each request it gets (its
# method, path, headers and body) as a line of JSON, then answers 200 with {}, as ntfy answers a
# message it took. While /var/lib/ntfy-mock/slow holds a number, it waits that many seconds after
# writing a request down and before answering, so the test can look at the process list while the
# service's request is still being made.
import http.server
import json
import threading
import time

LOG = "/var/lib/ntfy-mock/requests.jsonl"
SLOW = "/var/lib/ntfy-mock/slow"
lock = threading.Lock()


def delay():
    try:
        with open(SLOW) as f:
            return float(f.read().strip() or 0)
    except (OSError, ValueError):
        return 0


class Ntfy(http.server.BaseHTTPRequestHandler):
    def answer(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length).decode("utf-8", "replace")
        seen = {"method": self.command, "path": self.path, "headers": dict(self.headers), "body": body}
        with lock, open(LOG, "a") as f:
            f.write(json.dumps(seen) + "\n")
        time.sleep(delay())
        out = b"{}"
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    do_POST = answer
    do_PUT = answer
    do_GET = answer

    def log_message(self, *args):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", 8090), Ntfy).serve_forever()
