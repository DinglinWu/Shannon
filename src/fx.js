// 主题特效。只画在传进来的 canvas 上，搜索框里（scope: "window"）
// 和全屏覆盖窗口里（scope: "screen"，见 fx.html）共用同一套代码。
//
// 主题包本身不能带脚本：特效都写在这里，主题只按名字挑一个、再给参数。
// 想加新特效就在 EFFECTS 里加一项，并在 THEME_SPEC.md 里写明参数。

// 所有参数都在这里夹一遍范围：主题是数据，写错了、写离谱了都不能把程序拖垮
const clamp = (v, lo, hi, def) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : def;
};
const pick = (arr) => arr[(Math.random() * arr.length) | 0];
const rand = (a, b) => a + Math.random() * (b - a);
const range = (v, lo, hi, def) =>
  Array.isArray(v) && v.length === 2
    ? [clamp(v[0], lo, hi, def[0]), clamp(v[1], lo, hi, def[1])].sort((a, b) => a - b)
    : def;
const colorList = (v, def) =>
  Array.isArray(v) && v.length
    ? v.filter((c) => typeof c === "string" && /^#[0-9a-f]{3,8}$/i.test(c)).slice(0, 12)
    : def;

const DEFAULT_COLORS = ["#ffd23f", "#ff5a36", "#ff9f1c", "#fff3c4"];

// ---------- 烟花 ----------
// 全屏时从底部升起再炸开；在搜索框里空间太矮，直接原地炸开
function fireworks(ctx, w, h, p, duration, scope) {
  const colors = colorList(p.colors, DEFAULT_COLORS);
  const bursts = Math.round(clamp(p.bursts, 1, 12, scope === "screen" ? 6 : 3));
  const perBurst = Math.round(clamp(p.particles, 20, 160, 70));
  const gravity = clamp(p.gravity, 0, 3, 1);
  // 炸开半径跟画布大小走：全屏上大开大合，搜索框里小小一团
  const size = clamp(p.size, 0.3, 3, 1) * (scope === "screen" ? Math.min(w, h) / 900 : h / 260);

  const rockets = [];
  const sparks = [];
  // 发射时间铺在前 70% 的时长里，后面留给最后一朵散完
  const plan = Array.from({ length: bursts }, (_, i) => ({
    at: (duration * 0.7 * i) / Math.max(1, bursts - 1) + rand(-120, 120),
    done: false,
  }));

  function explode(x, y) {
    const main = pick(colors);
    const second = pick(colors);
    for (let i = 0; i < perBurst; i++) {
      const a = (Math.PI * 2 * i) / perBurst + rand(-0.08, 0.08);
      const v = rand(3.5, 6) * size * 60;
      sparks.push({
        x, y,
        vx: Math.cos(a) * v,
        vy: Math.sin(a) * v,
        life: 0,
        max: rand(0.9, 1.5),
        color: Math.random() < 0.75 ? main : second,
        r: rand(1.4, 2.4) * Math.max(0.6, Math.min(size, 1.4)),
      });
    }
  }

  return (t, dt) => {
    for (const b of plan) {
      if (b.done || t < b.at) continue;
      b.done = true;
      const x = rand(w * 0.15, w * 0.85);
      if (scope === "screen") {
        rockets.push({ x, y: h + 10, ty: rand(h * 0.15, h * 0.45), vy: -rand(h * 0.9, h * 1.2) });
      } else {
        explode(x, rand(h * 0.3, h * 0.7));
      }
    }

    ctx.globalCompositeOperation = "lighter";
    for (let i = rockets.length - 1; i >= 0; i--) {
      const r = rockets[i];
      r.y += r.vy * dt;
      ctx.fillStyle = "rgba(255, 230, 170, 0.9)";
      ctx.beginPath();
      ctx.arc(r.x, r.y, 2, 0, Math.PI * 2);
      ctx.fill();
      if (r.y <= r.ty) {
        explode(r.x, r.y);
        rockets.splice(i, 1);
      }
    }

    for (let i = sparks.length - 1; i >= 0; i--) {
      const s = sparks[i];
      s.life += dt;
      if (s.life >= s.max) {
        sparks.splice(i, 1);
        continue;
      }
      // 空气阻力 + 重力：先猛地散开，再慢慢垂落
      const drag = Math.pow(0.12, dt);
      s.vx *= drag;
      s.vy = s.vy * drag + 90 * gravity * size * dt;
      s.x += s.vx * dt;
      s.y += s.vy * dt;
      const k = 1 - s.life / s.max;
      ctx.globalAlpha = k * (0.75 + 0.25 * Math.random()); // 末尾闪烁
      // 画成顺着速度方向的一小段线，比圆点更像拉出来的火花
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.r * (0.5 + 0.5 * k);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(s.x - s.vx * 0.035, s.y - s.vy * 0.035);
      ctx.lineTo(s.x, s.y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";

    return plan.every((b) => b.done) && rockets.length === 0 && sparks.length === 0;
  };
}

// ---------- 通用飘落 / 升起 ----------
// 落叶、雪花、花瓣、孔明灯、红包雨……都是它：一张小图 + 方向 + 速度 + 摆动
function particles(ctx, w, h, p, duration, scope, images) {
  const up = p.direction === "up";
  const count = Math.round(clamp(p.count, 1, 120, scope === "screen" ? 40 : 14));
  const scale = scope === "screen" ? 1 : 0.55;
  const speed = range(p.speed, 10, 800, [60, 140]);
  const sizeR = range(p.size, 4, 96, [14, 26]);
  const sway = clamp(p.sway, 0, 80, 18) * scale;
  const spin = p.spin !== false;
  const colors = colorList(p.colors, DEFAULT_COLORS);
  // 进场时间铺在前 60% 的时长里
  const spawnWindow = duration * 0.6;

  const items = Array.from({ length: count }, () => {
    const s = rand(sizeR[0], sizeR[1]) * scale;
    return {
      at: rand(0, spawnWindow),
      x: rand(0, w),
      y: up ? h + s : -s,
      s,
      v: rand(speed[0], speed[1]) * scale,
      phase: rand(0, Math.PI * 2),
      freq: rand(0.6, 1.4),
      rot: rand(0, Math.PI * 2),
      vr: spin ? rand(-1.5, 1.5) : 0,
      img: images.length ? pick(images) : null,
      color: pick(colors),
      gone: false,
    };
  });

  return (t, dt) => {
    let alive = false;
    for (const it of items) {
      if (it.gone) continue;
      alive = true;
      if (t < it.at) continue;
      it.y += (up ? -1 : 1) * it.v * dt;
      it.rot += it.vr * dt;
      const x = it.x + Math.sin((t / 1000) * it.freq * 2 + it.phase) * sway;
      if (up ? it.y < -it.s * 2 : it.y > h + it.s * 2) {
        it.gone = true;
        continue;
      }
      // 时长快到时整体淡出，不必等每一片都飘出画面
      const fade = Math.min(1, Math.max(0, (duration + 600 - t) / 600));
      ctx.globalAlpha = fade;
      ctx.save();
      ctx.translate(x, it.y);
      ctx.rotate(it.rot);
      if (it.img && it.img.complete && it.img.naturalWidth) {
        const r = it.img.naturalHeight / it.img.naturalWidth;
        ctx.drawImage(it.img, -it.s / 2, (-it.s * r) / 2, it.s, it.s * r);
      } else {
        ctx.fillStyle = it.color;
        ctx.beginPath();
        ctx.arc(0, 0, it.s / 4, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.restore();
    }
    ctx.globalAlpha = 1;
    return !alive || t > duration + 600;
  };
}

const EFFECTS = { fireworks, particles };

export const EFFECT_NAMES = Object.keys(EFFECTS);

// 系统里开了"减少动画"就不放
export const reducedMotion = () =>
  window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// spec: { effect, duration, params }，imageUrls 是已经解析好的图片地址
// 返回 Promise，放完（或放不了）时 resolve
export function runFx(canvas, spec, imageUrls = [], scope = "window") {
  const make = EFFECTS[spec && spec.effect];
  if (!make || reducedMotion()) return Promise.resolve();

  const duration = clamp(spec.duration, 500, 8000, 2800);
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);

  const images = imageUrls.slice(0, 8).map((src) => {
    const im = new Image();
    im.src = src;
    return im;
  });

  const step = make(ctx, w, h, spec.params || {}, duration, scope, images);
  // 烟花要拖尾：每帧不全清，而是把上一帧整体擦淡一点。
  // 透明画布上只能用 destination-out 擦，填半透明黑会把背景染黑
  const trails = spec.effect === "fireworks";

  return new Promise((resolve) => {
    let start = null;
    let last = 0;
    const HARD_STOP = duration + 4000; // 保险：无论如何这么久之后一定收工

    function frame(now) {
      if (start === null) start = last = now;
      const t = now - start;
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;

      if (trails) {
        ctx.globalCompositeOperation = "destination-out";
        ctx.fillStyle = "rgba(0, 0, 0, 0.22)";
        ctx.fillRect(0, 0, w, h);
        ctx.globalCompositeOperation = "source-over";
      } else {
        ctx.clearRect(0, 0, w, h);
      }

      const finished = step(t, dt);
      if (finished || t > HARD_STOP) {
        ctx.clearRect(0, 0, w, h);
        resolve();
        return;
      }
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
  });
}
