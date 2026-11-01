import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest

spec = importlib.util.spec_from_file_location("notes_client", Path(__file__).resolve().parents[1] / "skills/shannon-notes/scripts/notes.py")
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)


class NotesSkillTests(unittest.TestCase):
    def test_request_response_roundtrip_preserves_unicode(self):
        with tempfile.TemporaryDirectory() as root:
            bridge = Path(root) / "notes-bridge"
            bridge.mkdir()
            received = []
            def serve():
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    for path in bridge.glob("*.request.json"):
                        received.append(json.loads(path.read_text(encoding="utf-8")))
                        result = path.with_name(path.name.replace(".request.json", ".response.json"))
                        result.write_text('{"ok":true,"data":{"saved":true}}', encoding="utf-8")
                        return
                    time.sleep(.01)
            thread = threading.Thread(target=serve)
            thread.start()
            request = {"action": "add", "text": "买牛奶\n保留引号：\"明天\""}
            result = client.call(root, request, timeout=3)
            thread.join()
            self.assertTrue(result["ok"])
            self.assertEqual(received[0]["request"], request)
            self.assertEqual(list(bridge.iterdir()), [])

    def test_timeout_removes_unclaimed_request(self):
        with tempfile.TemporaryDirectory() as root:
            bridge = Path(root) / "notes-bridge"
            bridge.mkdir()
            with self.assertRaisesRegex(RuntimeError, "超时"):
                client.call(root, {"action": "list"}, timeout=0)
            self.assertEqual(list(bridge.iterdir()), [])

    def test_missing_bridge_does_not_create_fake_service(self):
        with tempfile.TemporaryDirectory() as root:
            with self.assertRaisesRegex(RuntimeError, "接口不存在"):
                client.call(root, {"action": "list"})
            self.assertEqual(list(Path(root).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
