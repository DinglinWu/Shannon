"""Install fist dictation and the next-track facing fix in the companion project."""
from pathlib import Path
import os
import shutil

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get('GESTURE_MOUSE_HOME', r'E:\ironman\Gesture Recognition'))


def main():
    wrist = TARGET / 'wrist_controller.py'
    wrist_text = wrist.read_text(encoding='utf-8-sig')
    before_facing = "            elif horizontal > 0 and facing > .3:\n"
    after_facing = "            elif horizontal > 0:\n"
    if before_facing in wrist_text:
        wrist_text = wrist_text.replace(before_facing, after_facing)
    elif after_facing not in wrist_text:
        raise RuntimeError('Next-track wrist condition changed; refusing an ambiguous patch')
    wrist_tests = TARGET / 'tests' / 'test_wrist.py'
    test_text = wrist_tests.read_text(encoding='utf-8-sig')
    test_text = test_text.replace("dict(wrist_roll=-35,palm_facing=1),dict(wrist_roll=35,palm_facing=-1),",
                                  "dict(wrist_roll=-35,palm_facing=1),")
    readme = TARGET / 'README.md'
    readme_text = readme.read_text(encoding='utf-8-sig').replace(
        '向右摆腕约 28°且手心向前触发下一曲', '向右摆腕约 28°触发下一曲，无需特定手掌或手背朝向')
    if '握拳语音输入' not in readme_text:
        readme_text += '\n\n### 握拳语音输入\n\n闪念运行且手势已唤醒时，握拳稳定约半秒唤起搜索框并开始本地语音输入；保持握拳说话，松拳结束。握拳期间不会触发原有退出手势或捏合，Esc 也可结束语音。\n'
    app = TARGET / 'app.py'
    text = app.read_text(encoding='utf-8-sig')
    edits = [
        ('    from gestures import FeatureTracker, mirror_landmarks\n',
         '    from gestures import FeatureTracker, mirror_landmarks\n    from voice_point import is_voice_point, voice_safe_feature\n'),
        ("                              recognized_gesture=feedback.text,\n",
         "                              recognized_gesture=feedback.text,\n                              voice_point=bool(feature and feature.get('voice_point') and latest and time.monotonic() - latest[0] < .5),\n"),
        ('                controller.update(feature, time.monotonic())\n',
         "                if feature is not None:\n                    feature['voice_point'] = is_voice_point(feature, selected, w / h)\n                controller.update(voice_safe_feature(feature, controller), time.monotonic())\n"),
    ]
    if 'from voice_point import' not in text:
        for before, after in edits:
            if text.count(before) != 1:
                raise RuntimeError('Companion code changed; refusing an ambiguous patch: ' + before.strip())
            text = text.replace(before, after)
        backup = TARGET / 'app.py.before-shannon-voice'
        if not backup.exists():
            shutil.copy2(app, backup)
    text = text.replace('controller.update(voice_safe_feature(feature), time.monotonic())',
                        'controller.update(voice_safe_feature(feature, controller), time.monotonic())')
    app.write_text(text, encoding='utf-8')
    shutil.copy2(ROOT / 'integrations/gesture-voice/voice_point.py', TARGET / 'voice_point.py')
    for path, contents in ((wrist, wrist_text), (wrist_tests, test_text), (readme, readme_text)):
        backup = path.with_suffix(path.suffix + '.before-shannon-fist')
        if not backup.exists():
            shutil.copy2(path, backup)
        path.write_text(contents, encoding='utf-8')
    print('Gesture voice telemetry installed; restart gesture recognition to apply.')


if __name__ == '__main__':
    main()
