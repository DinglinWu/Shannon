// 纯计时逻辑：按时间戳累计，避免后台窗口节流造成计时漂移。
export function parseDuration(value) {
  const text = String(value).trim().toLowerCase();
  let seconds;
  if (/^\d+(?::\d{1,2}){1,2}$/.test(text)) {
    const parts = text.split(":").map(Number);
    if (parts.slice(1).some((n) => n >= 60)) return null;
    seconds = parts.reduce((sum, n) => sum * 60 + n, 0);
  } else if (/^\d+(?:\.\d+)?$/.test(text)) {
    seconds = Number(text) * 60; // 不带单位时按分钟
  } else {
    const match = text.match(/^(?:(\d+)h)?\s*(?:(\d+)m)?\s*(?:(\d+)s)?$/);
    if (!match || !text) return null;
    seconds = Number(match[1] || 0) * 3600 + Number(match[2] || 0) * 60 + Number(match[3] || 0);
  }
  return Number.isFinite(seconds) && Math.round(seconds) >= 1 && seconds <= 86400
    ? Math.round(seconds) * 1000 : null;
}

export function formatTime(ms, countdown = false) {
  const total = Math.max(0, countdown ? Math.ceil(ms / 1000) : Math.floor(ms / 1000));
  const pad = (n) => String(n).padStart(2, "0");
  const hours = Math.floor(total / 3600);
  const rest = `${pad(Math.floor(total / 60) % 60)}:${pad(total % 60)}`;
  return hours ? `${pad(hours)}:${rest}` : rest;
}

export class Clock {
  constructor(mode, duration = 0) {
    this.mode = mode;
    this.duration = duration;
    this.elapsed = 0;
    this.startedAt = null;
  }
  resume(now = Date.now()) {
    if (this.startedAt === null) this.startedAt = now;
  }
  pause(now = Date.now()) {
    this.elapsed = this.elapsedAt(now);
    this.startedAt = null;
  }
  elapsedAt(now = Date.now()) {
    return this.elapsed + (this.startedAt === null ? 0 : Math.max(0, now - this.startedAt));
  }
  remaining(now = Date.now()) {
    return Math.max(0, this.duration - this.elapsedAt(now));
  }
}
