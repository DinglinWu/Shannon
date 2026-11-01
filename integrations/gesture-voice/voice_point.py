"""Closed-fist dictation. Legacy voice_point names preserve the companion bridge ABI."""
import time


def is_voice_point(feature, landmarks, aspect):
    if not feature or not landmarks or not feature.get('control_valid'):
        return False
    # Thumb posture and palm/back orientation vary naturally in a closed fist.
    # Require all four fingers folded, rejecting uncertain or extended fingers.
    return feature.get('finger_states', [])[1:] == [False, False, False, False]


def voice_safe_feature(feature, controller=None, now=None):
    """Keep tracking/wake timers; reserve the fist for dictation, including exit_pose."""
    if not feature or not feature.get('voice_point'):
        return feature
    # A deliberate held dictation gesture must not be cut off by the usual
    # six-second shortcut window. Never wake an inactive controller here.
    if controller is not None and controller.recognition_active:
        controller.active_until = max(controller.active_until,
            (time.monotonic() if now is None else now) + controller.c['recognition_seconds'])
    return feature | dict(left=10, right=10, point=False, scroll=False,
                          pinch_eligible=False, control_pose=None, open=False, fist=False, exit_pose=False)
