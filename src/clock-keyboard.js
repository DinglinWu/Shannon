// 文本仍可正常编辑；只有无选区且光标已经到边界时才切换模式。
export function modeForArrow(mode, key, selection = null) {
  const target = key === "ArrowLeft" ? "down" : key === "ArrowRight" ? "up" : null;
  if (!target || target === mode) return null;
  if (selection) {
    const { start, end, length } = selection;
    if (start !== end) return null;
    if (key === "ArrowLeft" ? start !== 0 : end !== length) return null;
  }
  return target;
}
