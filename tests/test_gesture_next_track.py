"""Regression tests against the installed companion wrist controller, no camera/input."""
import importlib.util
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(os.environ.get('GESTURE_MOUSE_HOME', r'E:\ironman\Gesture Recognition'))
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location('companion_wrist_tests', ROOT / 'tests/test_wrist.py')
wrist_tests = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrist_tests)


class NextTrackFacingTests(unittest.TestCase):
    def test_next_track_ignores_palm_back_or_edge_facing_and_fires_once(self):
        for facing in (-1, -.1, 0, .1, 1):
            with self.subTest(facing=facing):
                case = wrist_tests.WristTests()
                case.setUp()
                case.hold(.5, wrist_roll=35, palm_facing=facing)
                self.assertEqual(case.controller.last_action['action'], 'next_track')
                count = len(case.mouse.events)
                case.hold(.4, wrist_roll=35, palm_facing=facing)
                self.assertEqual(len(case.mouse.events), count)

    def test_previous_track_keeps_existing_facing_rule(self):
        case = wrist_tests.WristTests()
        case.setUp()
        case.hold(.5, wrist_roll=-35, palm_facing=1)
        self.assertIsNone(case.controller.last_action)
        case.hold(.5, wrist_roll=-35, palm_facing=-1)
        self.assertEqual(case.controller.last_action['action'], 'previous_track')


if __name__ == '__main__':
    unittest.main()
