// 农历主题使用明确日期；每年重复的节日按年度区分，跨年区间归属起始年。
export function themeActivationKey(theme, day) {
  if (!theme) return null;
  const when = theme.when;
  if (!when) return theme.id;
  if (when.from.length === 10) return `${theme.id}:${when.from}:${when.to}`;
  let year = Number(day.slice(0, 4));
  if (when.from > when.to && day.slice(5) <= when.to) year--;
  return `${theme.id}:${year}:${when.from}:${when.to}`;
}
