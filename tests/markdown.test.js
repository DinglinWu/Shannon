import test from 'node:test';
import assert from 'node:assert/strict';
import { renderMarkdown } from '../src/markdown.js';

test('renders dollar and backslash inline formulas with accessible MathML', () => {
  for (const source of [String.raw`结果 $x^2 + \frac{1}{2}$。`, String.raw`结果 \(x^2 + \frac{1}{2}\)。`]) {
    const html = renderMarkdown(source);
    assert.match(html, /class="katex"/);
    assert.match(html, /<math /);
    assert.match(html, /<mfrac>/);
  }
});

test('renders display formulas, matrices, and aligned equations', () => {
  for (const source of [
    '$$\nx^2+y^2=z^2\n$$',
    String.raw`\[\begin{aligned} a &= b+c \\ d &= e \end{aligned}\]`,
    String.raw`$$\begin{pmatrix}1&2\\3&4\end{pmatrix}$$`,
  ]) {
    const html = renderMarkdown(source);
    assert.match(html, /class="math-display"/);
    assert.match(html, /katex-display/);
    assert.doesNotMatch(html, /katex-error/);
  }
});

test('math is protected from Markdown emphasis and works in lists and tables', () => {
  for (const source of [String.raw`- $x_i + y_j$`, '| 公式 |\n| --- |\n| $x_i$ |']) {
    const html = renderMarkdown(source);
    assert.match(html, /class="katex"/);
    assert.doesNotMatch(html, /<em>/);
  }
});

test('code spans, fenced code, escaped dollars, and currency remain literal', () => {
  for (const source of ['`$x^2$`', '```latex\n$$x^2$$\n```', String.raw`价格 \$5 和 \$10`, '价格 $5 和 $10', '费用 $ 5，余额 $ 10']) {
    assert.doesNotMatch(renderMarkdown(source), /class="katex"/);
  }
});

test('incomplete streamed formulas stay text and render when completed', () => {
  const source = String.raw`答案：\[\frac{1}{2}`;
  assert.doesNotMatch(renderMarkdown(source), /class="katex"/);
  assert.match(renderMarkdown(source + String.raw`\]`), /class="katex"/);
  assert.doesNotMatch(renderMarkdown('答案：$x^2'), /class="katex"/);
  assert.match(renderMarkdown('答案：$x^2$'), /class="katex"/);
});

test('invalid TeX is readable and cannot break later Markdown rendering', () => {
  assert.match(renderMarkdown(String.raw`$\notARealCommand{x}$`), /notARealCommand/);
  assert.match(renderMarkdown(String.raw`$\frac{1$`), /katex-error/);
  assert.match(renderMarkdown('**仍可继续回答**'), /<strong>仍可继续回答<\/strong>/);
});

test('formula HTML commands and raw model HTML cannot create executable markup', () => {
  const html = renderMarkdown(String.raw`$\href{javascript:alert(1)}{click}$` + '\n\n<img src=x onerror=alert(1)>');
  assert.doesNotMatch(html, /<a[^>]+href="javascript:/);
  assert.doesNotMatch(html, /<img /);
  assert.match(html, /&lt;img /);
});

test('cached formulas give stable output on subsequent streaming renders', () => {
  const formula = String.raw`$\sqrt{x+1}$`;
  const first = renderMarkdown(formula);
  assert.equal(renderMarkdown(formula), first);
  assert.match(renderMarkdown(formula + ' 后续内容'), /后续内容/);
});
