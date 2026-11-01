export function greetingLayout(anchor, viewport) {
  const width = Math.max(0, Math.min(620, anchor.width, viewport.width - 32));
  const height = Math.min(104, anchor.top - 12);
  if (width < 80 || height < 28) return null;
  return {
    width, height,
    left: Math.max(16, Math.min(viewport.width - width - 16, anchor.left + (anchor.width - width) / 2)),
    top: anchor.top - height - 12,
  };
}

// 寄语在透明特效窗口里绘制，既不改搜索框位置，也不遮住输入内容。
export function renderGreeting(spec) {
  const layout = greetingLayout(spec.anchor, { width: innerWidth, height: innerHeight });
  if (!layout) return { finished: Promise.resolve(), dismiss() {} };
  const el = document.createElement("section");
  el.className = "festival-greeting";
  el.classList.toggle("compact", layout.height < 76);
  el.setAttribute("role", "status");
  el.setAttribute("aria-label", spec.text);
  Object.assign(el.style, {
    left: layout.left + "px", top: layout.top + "px",
    width: layout.width + "px", height: layout.height + "px",
  });
  el.style.setProperty("--greeting-accent", spec.accent);
  el.style.setProperty("--greeting-bg", spec.background);
  const image = (url, className) => {
    const img = document.createElement("img");
    img.src = url;
    img.alt = "";
    img.className = className;
    img.draggable = false;
    el.appendChild(img);
  };
  if (spec.pattern) image(spec.pattern, "greeting-pattern");
  if (spec.badge) {
    image(spec.badge, "greeting-badge left");
    image(spec.badge, "greeting-badge right");
  }
  for (let i = 0; i < Math.min(4, (spec.ornaments || []).length ? 4 : 0); i++) {
    image(spec.ornaments[i % spec.ornaments.length], `greeting-ornament ornament-${i}`);
  }
  const text = document.createElement("div");
  text.className = "greeting-text";
  text.setAttribute("aria-hidden", "true");
  [...spec.text].forEach((ch, i) => {
    const span = document.createElement("span");
    span.textContent = ch;
    span.style.animationDelay = `${160 + i * 70}ms`;
    text.appendChild(span);
  });
  el.appendChild(text);
  document.body.appendChild(el);
  let resolve;
  let timer;
  let removing = false;
  const finished = new Promise((done) => { resolve = done; });
  function dismiss(instant = false) {
    if (removing && !instant) return;
    removing = true;
    clearTimeout(timer);
    if (instant) { el.remove(); resolve(); return; }
    el.classList.add("out");
    timer = setTimeout(() => { el.remove(); resolve(); }, 460);
  }
  timer = setTimeout(() => dismiss(), Math.max(spec.duration, 160 + [...spec.text].length * 70 + 480 + 2000));
  return { finished, dismiss };
}
