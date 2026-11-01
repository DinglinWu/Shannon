// 弹簧动画，参数用苹果（SwiftUI / Core Animation）的那套说法：
//   response：快慢，一次"来回"大约多少秒（越小越快）
//   bounce：  弹性，0 是不弹、刚好停住（临界阻尼）；0.1~0.15 是苹果界面里常见的那一丝回弹；
//             0.3 以上就是明显的"弹一下"，只适合俏皮的场合
// 换算成物理量：ω = 2π / response，阻尼比 ζ = 1 − bounce。
//
// 中途换目标时从当前位置和速度接着走，不会跳。用解析解而不是数值积分，掉帧也不会越算越偏。

const reduced = () =>
  window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function spring({ response = 0.35, bounce = 0, onUpdate }) {
  let omega, zeta, a, wd;
  function setParams(r, b) {
    omega = (2 * Math.PI) / r;
    zeta = Math.min(1, Math.max(0.05, 1 - b));
    a = zeta * omega;
    wd = zeta < 1 ? omega * Math.sqrt(1 - zeta * zeta) : 0;
  }
  setParams(response, bounce);

  let target = 0;
  let x0 = 0; // 起点相对目标的偏移
  let v0 = 0;
  let t0 = 0;
  let raf = 0;
  let ready = false;

  // t 时刻的 [位置, 速度, 离目标的偏移]
  function state(t) {
    const tau = (t - t0) / 1000;
    const e = Math.exp(-a * tau);
    let d;
    let v;
    if (zeta < 1) {
      // 欠阻尼：衰减振荡
      const B = (v0 + a * x0) / wd;
      const c = Math.cos(wd * tau);
      const s = Math.sin(wd * tau);
      d = e * (x0 * c + B * s);
      v = e * ((B * wd - a * x0) * c - (x0 * wd + a * B) * s);
    } else {
      // 临界阻尼：最快地停住，不冲过头
      const C = v0 + omega * x0;
      d = (x0 + C * tau) * e;
      v = (v0 - omega * C * tau) * e;
    }
    return [target + d, v, d];
  }

  function frame(t) {
    const [x, v, d] = state(t);
    if (Math.abs(d) < 0.05 && Math.abs(v) < 1) {
      raf = 0;
      onUpdate(target);
      return;
    }
    onUpdate(x);
    raf = requestAnimationFrame(frame);
  }

  return {
    // immediate：直接到位不做动画（第一次出现、藏起来又出现时用）
    // params：这一次换一组 { response, bounce }，从当前位置和速度接着走
    to(next, immediate = false, params = null) {
      if (!ready || immediate || reduced()) {
        ready = true;
        target = next;
        if (raf) cancelAnimationFrame(raf);
        raf = 0;
        onUpdate(next);
        return;
      }
      if (!raf && next === target) return;
      const now = performance.now();
      const [x, v] = raf ? state(now) : [target, 0]; // 先按旧参数算出此刻的状态
      if (params) setParams(params.response, params.bounce);
      target = next;
      x0 = x - next;
      v0 = v;
      t0 = now;
      if (!raf) raf = requestAnimationFrame(frame);
    },
    // 下次 to() 直接到位
    reset() {
      ready = false;
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
    },
  };
}
