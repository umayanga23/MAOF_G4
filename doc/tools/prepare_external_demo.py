"""Create an isolated two-external-agent workflow using real service commands."""
import argparse
import uuid
from prepare_feature_lab import Bridge, ROOT, task


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--a2a", action="store_true")
    parser.add_argument("--endpoint", help="Full execution URL; default is the included local sample service")
    parser.add_argument("--token-env", help="ORCHESTRATION_AGENT_... variable containing a bearer token")
    args = parser.parse_args()
    endpoint = args.endpoint or ("http://127.0.0.1:8765/a2a" if args.a2a else "http://127.0.0.1:8765/run")
    directory = ROOT / ".cache" / ("external-demo-" + uuid.uuid4().hex)
    directory.mkdir(parents=True, exist_ok=False)
    bridge = Bridge(directory)
    try:
        state = bridge.call("snapshot")["workspace"]
        base = state["agents"][0]
        for identity, name, instruction, priority in [
            ("external-reader", "External Researcher", "Describe the topic and compare two approaches.", 2),
            ("external-writer", "External Writer", "Write a brief summary from completed-task findings.", 1),
        ]:
            agent = dict(base, id=identity, name=name, role=instruction, instructions=instruction,
                         model="", priority=priority, memory_mb=64, read_permission="ask",
                         external={"protocol": "a2a_json_rpc" if args.a2a else "http_json",
                                   "endpoint": endpoint, "timeout_seconds": 60,
                                   "bearer_token_env": args.token_env, "send_context": True})
            bridge.command("save_agent", agent=agent)
        bridge.command("import_document", name="feature-lab-notes.md",
                       text=(ROOT / "examples/feature-lab-notes.md").read_text(encoding="utf-8"))
        document = bridge.call("snapshot")["workspace"]["documents"][-1]["id"]
        bridge.command("save_workflow", workflow={"id": "external-team", "name": "External agent team", "tasks": [
            task("research", "Read Orchard notes and explain the approaches", "external-reader", document=document),
            task("report", "Summarize the completed research", "external-writer", ["research"], True),
        ]})
        settings = state["settings"]
        settings.update(mode="ollama", worker_slots=1, output_tokens=128)
        bridge.command("save_settings", settings=settings)
    finally:
        bridge.close()
    print(f"Practice workspace: {directory}")
    print(f"$env:ORCHESTRATION_LAB_DATA_DIR = '{directory}'")
    print(f"& '{ROOT / 'target/debug/orchestration-desktop.exe'}'")
    print("Overview: select External agent team. Prompt: Compare the supplied Orchard approaches; preserve SAMPLE42.")
    print("Approve the synthetic document read, then the local report. No Ollama is required for this workflow.")


if __name__ == "__main__":
    main()
