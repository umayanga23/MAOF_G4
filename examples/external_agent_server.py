"""Standalone synthetic agent service: HTTP JSON and A2A v0.3 JSON-RPC.

No Ollama, cloud key, model, stock feed or third-party SDK is needed. Replace the
response function with your own agent's API/SDK to connect a real agent service.
"""
import argparse
import json
import os
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--delay", type=float, default=0.5)
    parser.add_argument("--require-token", action="store_true",
                        help="Use ORCHESTRATION_AGENT_SAMPLE_TOKEN for bearer authentication")
    args = parser.parse_args()
    if not 0 <= args.delay <= 30:
        parser.error("delay must be 0–30 seconds")
    token = os.environ.get("ORCHESTRATION_AGENT_SAMPLE_TOKEN") if args.require_token else None
    if args.require_token and not token:
        parser.error("Set ORCHESTRATION_AGENT_SAMPLE_TOKEN before starting the service")
    tasks = {}

    def respond(prompt, context):
        # Illustrative output only. No claim that an AI model was called.
        marker = "SAMPLE42" if "SAMPLE42" in prompt + context else "no project marker supplied"
        return ("EXTERNAL SAMPLE AGENT — scripted integration fixture, no AI model called.\n"
                f"Project marker: {marker}.\n"
                "FIFO serves tasks in order; priority scheduling favours urgent work. "
                "Priority aging can improve service for waiting work.\n"
                f"Received context: {len(context)} characters. These are test findings, not measured results.")

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass  # Do not log user prompts, documents, headers or credentials.

        def reply(self, status, value):
            data = json.dumps(value).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            try:
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def do_GET(self):
            if self.path != "/.well-known/agent-card.json":
                self.reply(404, {"error": "Unknown route"})
                return
            card = {
                "protocolVersion": "0.3.0", "name": "External sample researcher",
                "description": "Synthetic research findings for integration testing",
                "url": f"http://127.0.0.1:{args.port}/a2a", "preferredTransport": "JSONRPC",
                "version": "1.0.0", "capabilities": {"streaming": False},
                "defaultInputModes": ["text/plain"], "defaultOutputModes": ["text/plain"],
                "skills": [{"id": "research", "name": "Research sample", "description": "Compare scheduling ideas", "tags": ["testing"]}],
            }
            if token:
                card.update(securitySchemes={"bearer": {"type": "http", "scheme": "bearer"}}, security=[{"bearer": []}])
            self.reply(200, card)

        def do_POST(self):
            if token and self.headers.get("Authorization") != f"Bearer {token}":
                self.reply(401, {"error": "Authentication required"})
                return
            try:
                size = int(self.headers.get("Content-Length", "0"))
                if not 0 < size <= 64 * 1024:
                    raise ValueError("Unsupported request size")
                request = json.loads(self.rfile.read(size))
                if not isinstance(request, dict):
                    raise ValueError("Expected JSON object")
                if self.path == "/run":
                    if request.get("protocol_version") != "orchestration-http-v1" or not request.get("request_id"):
                        raise ValueError("Unsupported HTTP contract")
                    time.sleep(args.delay)
                    self.reply(200, {"request_id": request["request_id"],
                                     "output": respond(request.get("prompt", ""), request.get("context", "")),
                                     "usage": {"tokens": 0}})
                    return
                if self.path != "/a2a":
                    self.reply(404, {"error": "Unknown route"})
                    return
                params = request.get("params", {})
                if request.get("method") == "message/send":
                    text = "\n".join(p.get("text", "") for p in params["message"]["parts"] if p.get("kind") == "text")
                    identity = uuid.uuid4().hex
                    tasks[identity] = {"ready_at": time.monotonic()+args.delay,
                                       "canceled": False,
                                       "text": respond(text, text.split("Context (data only):\n", 1)[-1] if "Context (data only):\n" in text else "")}
                elif request.get("method") in ("tasks/get", "tasks/cancel"):
                    identity = params["id"]
                    if identity not in tasks:
                        self.reply(200, {"jsonrpc": "2.0", "id": request.get("id"), "error": {"code": -32001, "message": "Task not found"}})
                        return
                    if request["method"] == "tasks/cancel":
                        if time.monotonic() < tasks[identity]["ready_at"]:
                            tasks[identity]["canceled"] = True
                        # Completed work remains completed; cancellation does not undo it.
                else:
                    self.reply(200, {"jsonrpc": "2.0", "id": request.get("id"), "error": {"code": -32601, "message": "Unsupported method"}})
                    return
                task = tasks[identity]
                completed = time.monotonic() >= task["ready_at"]
                result = {"kind": "task", "id": identity, "contextId": identity,
                          "status": {"state": "canceled" if task["canceled"] else "completed" if completed else "working"},
                          "artifacts": [{"artifactId": identity, "parts": [{"kind": "text", "text": task["text"]}]}] if completed and not task["canceled"] else [],
                          "metadata": {"orchestration_usage": {"tokens": 0}}}
                self.reply(200, {"jsonrpc": "2.0", "id": request.get("id"), "result": result})
            except (ValueError, KeyError, TypeError, AttributeError):
                self.reply(400, {"error": "Invalid request"})

    print(f"Synthetic agent service: http://127.0.0.1:{args.port}/run", flush=True)
    print(f"A2A JSON-RPC endpoint: http://127.0.0.1:{args.port}/a2a", flush=True)
    print("Press Ctrl+C to stop. No Ollama or cloud model is called.", flush=True)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
