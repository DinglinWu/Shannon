const paths = {
  volume_control: '<path d="M8 20V7a2 2 0 0 1 4 0v7M12 9V4a2 2 0 0 1 4 0v12l-3 5H9l-4-6M21 4v16m-2-14 2-2 2 2m-4 12 2 2 2-2"/>',
  track_control: '<path d="M8 18V8m3 9V6m3 11V7m3 11V9M8 18l3 3h4l3-4M2 4h20M5 1 2 4l3 3m14-6 3 3-3 3"/>',

  play_pause: '<path d="m3 5 9 7-9 7ZM16 5v14M21 5v14"/>',
  previous_track: '<path d="M5 5v14m14-14L8 12l11 7Z"/>',
  next_track: '<path d="M19 5v14M5 5l11 7-11 7Z"/>',
  volume_up: '<path d="m11 4-6 5H2v6h3l6 5ZM15 8a6 6 0 0 1 0 8M18 4a11 11 0 0 1 0 16"/>',
  volume_down: '<path d="m11 4-6 5H2v6h3l6 5ZM15 8a6 6 0 0 1 0 8"/>',
  mute: '<path d="m11 4-6 5H2v6h3l6 5ZM16 9l6 6m0-6-6 6"/>',
  page_next: '<path d="M13 3H4v18h9m3-14 5 5-5 5m5-5H9"/>',
  page_previous: '<path d="M11 3h9v18h-9M8 7l-5 5 5 5m-5-5h12"/>',
  launcher: '<circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/>',

  hand: '<path d="M8 13V5a2 2 0 0 1 4 0v7M12 7a2 2 0 0 1 4 0v5M16 9a2 2 0 0 1 4 0v6a7 7 0 0 1-7 7h-1a6 6 0 0 1-5-3l-4-6a2 2 0 0 1 3-2l2 2"/>',
  pinch: '<path d="M6 19c-2-2-2-5-1-7l3-5a2 2 0 0 1 3 2l-2 4M9 13l4-3a3 3 0 0 1 4 4l-3 3M6 19l4 2h4l5-5"/><circle cx="17" cy="6" r="3"/>',
  scan: '<path d="M3 8V4h4M17 4h4v4M3 16v4h4M17 20h4v-4M9 15V9a1.5 1.5 0 0 1 3 0v4M12 11a1.5 1.5 0 0 1 3 0v4l-2 3h-3l-3-5"/>',
  pause: '<path d="M9 5v14M15 5v14"/>',
  play: '<path d="m8 5 11 7-11 7Z"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  camera: '<rect x="3" y="6" width="18" height="13" rx="3"/><path d="m8 6 1-2h6l1 2"/><circle cx="12" cy="12.5" r="3"/>',
  alert: '<path d="M10.3 4a2 2 0 0 1 3.4 0l8 14a2 2 0 0 1-1.7 3H4a2 2 0 0 1-1.7-3Z"/><path d="M12 9v4M12 17h.01"/>',
  scroll: '<path d="M8 7V4m0 0L5 7m3-3 3 3M16 17v3m0 0-3-3m3 3 3-3"/><rect x="6" y="9" width="12" height="6" rx="3"/>',
  cursor: '<path d="m5 3 14 10-7 1-3 7Z"/>',
  swipe_left: '<path d="M20 7H4m0 0 4-4M4 7l4 4M9 20v-5a2 2 0 0 1 4 0v3M13 16a2 2 0 0 1 4 0v4"/>',
  swipe_right: '<path d="M4 7h16m0 0-4-4m4 4-4 4M7 20v-5a2 2 0 0 1 4 0v3M11 16a2 2 0 0 1 4 0v4"/>',
  swipe_up: '<path d="M7 20V4m0 0L3 8m4-4 4 4M16 20v-7a2 2 0 0 1 4 0v7M13 17l3 3"/>',
  swipe_down: '<path d="M7 4v16m0 0-4-4m4 4 4-4M16 17V9a2 2 0 0 1 4 0v9M13 14l3 3"/>',
};
export function icon(name) { return `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[name] || paths.hand}</svg>`; }

export const actionNames = {
  scroll_up: '页面向上滚动', scroll_down: '页面向下滚动', play_pause: '播放 / 暂停', previous_track: '上一曲', next_track: '下一曲',
  volume_up: '调高音量', volume_down: '调低音量', mute: '静音 / 取消静音',
  page_next: '下一页', page_previous: '上一页', launcher: '唤起搜索框', none: '已禁用',
};

export const trackingHints = {
  'ARMING TRACKS': '四指伸直，保持手掌朝向镜头',
  'ARMED TRACKS': '四指已就位，左右摆腕切歌、上下摆腕滚动',
  'ARMING VOLUME': '伸出食指和中指，转动手腕调音量',
  'PINCH': '松开手指确认播放 / 暂停',
  'DRAG': '保持捏合拖拽，松开结束',
  'SCROLL': '上下移动手掌滚动页面',
  'MOVE': '移动食指控制光标',
};

export function statusView(status = {}, now = Date.now() / 1000) {
  const tracking = status.tracking || '';
  const common = { active: Boolean(status.recognition_active), camera: status.camera_name || '等待摄像头',
    fps: Number.isFinite(status.fps) ? `${Math.round(status.fps)} FPS` : '— FPS',
    hand: status.hand === 'Left' ? '左手' : '右手',
    simulated: Boolean(status.dry_run), remaining: Math.max(0, Number(status.active_remaining) || 0) };
  const view = (title, subtitle, glyph, badge, tone) => ({ ...common, title, subtitle, glyph, badge, tone });
  const countdown = status.dry_run ? '模拟' : `${Math.ceil(common.remaining)}秒`;
  if (status.phase === 'error') return view('设备异常', status.error || '', 'alert', '', 'error');
  if (status.telemetry_stale) return view('连接中断', '', 'alert', '', 'error');
  if (!status.phase || status.phase === 'starting') return view('启动中', '', 'scan', '', 'starting');
  if (status.phase === 'stopped' || !common.active) return view('未唤醒', '', 'hand', '', 'waiting');
  if (tracking === 'NO HAND') return view('未检测到手', '', 'scan', countdown, 'waiting');
  if (tracking === 'RECOVERING') return view('识别中', '', 'scan', countdown, 'starting');
  if (tracking.startsWith('RELEASE PINCH') || tracking.startsWith('PINCH HELD')) return view('松开手指', '', 'pinch', countdown, 'waiting');
  const last = status.last_action;
  if (last && Number.isFinite(last.at) && now - last.at >= 0 && now - last.at < 1.5) {
    return view(actionNames[last.action] || last.action, '',
      Object.hasOwn(paths, last.action) ? last.action : last.gesture, countdown, 'action');
  }
  if (tracking === 'OUTSIDE CONTROL AREA') return view('移入控制区域', '', 'scan', countdown, 'waiting');
  if (tracking === 'COOLDOWN') return view('冷却中', '', 'scan', countdown, 'waiting');
  if (tracking.endsWith('WRIST')) return view('摆腕识别', '左背上一曲 · 右摆下一曲 · 上下滚动', 'track_control', countdown, 'active');
  if (tracking === 'RETURN WRIST TO CENTER') return view('请将手腕回中', '保持手腕位置，再做摆动', 'hand', countdown, 'waiting');
  if (tracking.endsWith('VOLUME')) return view('双指旋钮', '', 'volume_control', countdown, tracking.startsWith('ARMING') ? 'starting' : 'active');
  if (tracking.endsWith('TRACKS')) return view('四指横挥', '', 'track_control', countdown, tracking.startsWith('ARMING') ? 'starting' : 'active');
  if (tracking.startsWith('PINCH')) return view('捏合中', '松开确认', 'pinch', countdown, 'active');
  if (tracking === 'DRAG') return view('拖拽中', '', 'pinch', countdown, 'active');
  if (tracking === 'SCROLL') return view('滚动中', '', 'scroll', countdown, 'active');
  if (tracking.startsWith('RELAX HAND')) return view('已挥手', '', 'hand', countdown, 'waiting');
  if (tracking.startsWith('OPEN PALM')) return view('挥手中', '', 'hand', countdown, 'active');
  if (tracking === 'MOVE') return view('移动中', '', 'cursor', countdown, 'active');
  return view('已就绪', '', 'hand', countdown, 'active');
}
