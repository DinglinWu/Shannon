import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location('voice_point', Path(__file__).resolve().parents[1] / 'integrations/gesture-voice/voice_point.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FistTests(unittest.TestCase):
    def sample(self):
        points = [SimpleNamespace(z=0) for _ in range(21)]
        points[8].z = -.1
        return dict(control_valid=True, finger_states=[False, False, False, False, False],
                    pinch_distance=.8, palm_size=.15), points

    def test_closed_fist_accepts_any_direction_and_thumb_position(self):
        feature, points = self.sample()
        self.assertTrue(module.is_voice_point(feature, points, 1.3))
        points[8].z = 0
        self.assertTrue(module.is_voice_point(feature, points, 1.3))
        points[8].z = .1
        self.assertTrue(module.is_voice_point(feature, points, 1.3))
        self.assertTrue(module.is_voice_point(feature | dict(finger_states=[True, False, False, False, False], pinch_distance=.1), points, 1.3))

    def test_no_hand_open_palm_point_volume_uncertainty_and_outside_do_not_trigger(self):
        feature, points = self.sample()
        for patch in [dict(control_valid=False), dict(finger_states=[False, True, False, False, False]),
                      dict(finger_states=[False, None, False, False, False]),
                      dict(finger_states=[False, True, True, False, False]),
                      dict(finger_states=[False, True, True, True, True])]:
            self.assertFalse(module.is_voice_point(feature | patch, points, 1.3))
        self.assertFalse(module.is_voice_point(None, None, 1.3))

    def test_fist_keeps_tracking_without_mouse_shortcuts_or_exiting(self):
        feature, _ = self.sample()
        safe = module.voice_safe_feature(feature | dict(voice_point=True))
        self.assertFalse(safe['point'])
        self.assertFalse(safe['pinch_eligible'])
        self.assertFalse(safe['exit_pose'])
        self.assertEqual(safe['right'], 10)
        self.assertEqual(safe['finger_states'], feature['finger_states'])

    def test_held_point_extends_wake_window_but_never_wakes_inactive_controller(self):
        feature, _ = self.sample()
        controller = SimpleNamespace(recognition_active=True, active_until=49, c={'recognition_seconds': 6})
        module.voice_safe_feature(feature | dict(voice_point=True), controller, now=50)
        self.assertEqual(controller.active_until, 56)
        controller.recognition_active = False
        controller.active_until = None
        module.voice_safe_feature(feature | dict(voice_point=True), controller, now=60)
        self.assertIsNone(controller.active_until)


if __name__ == '__main__':
    unittest.main()
