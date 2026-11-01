import { Marked } from 'marked';
import katex from 'katex';

const formulas = new Map();
const escape = text => text.replace(/[&<>"']/g, char => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[char]);

function escapedAt(source, index) {
  let slashes = 0;
  while (index > 0 && source[--index] === '\\') slashes++;
  return slashes % 2 !== 0;
}

function readMath(source) {
  const open = ['$$', '\\[', '\\(', '$'].find(delimiter => source.startsWith(delimiter));
  if (!open) return;
  const display = open === '$$' || open === '\\[';
  const close = open === '\\[' ? '\\]' : open === '\\(' ? '\\)' : open;
  if (open === '$' && /\s/.test(source[1] || ' ')) return;
  let end = open.length;
  while ((end = source.indexOf(close, end)) !== -1) {
    if (escapedAt(source, end) || (open === '$' &&
        (source[end - 1] === '$' || source[end + 1] === '$' ||
         /\s/.test(source[end - 1]) || /\d/.test(source[end + 1] || '')))) {
      end += close.length;
      continue;
    }
    const text = source.slice(open.length, end);
    if (!display && /\n/.test(text)) return;
    if (!text.trim()) return;
    return { raw: source.slice(0, end + close.length), text: text.trim(), display };
  }
  // An unfinished streamed formula stays readable until its closing delimiter.
}

function renderMath(token) {
  // Bound both expansion work and the cache retained across streaming chunks.
  if (token.text.length > 20000) return escape(token.raw);
  const key = `${token.display}:${token.text}`;
  if (formulas.has(key)) return formulas.get(key);
  let html;
  try {
    html = katex.renderToString(token.text, {
      displayMode: token.display,
      output: 'htmlAndMathml',
      throwOnError: false,
      trust: false,
      strict: 'ignore',
      maxExpand: 1000,
      maxSize: 20,
    });
  } catch {
    // Unexpected parser limits must not interrupt an entire streamed answer.
    return `<span class="katex-error">${escape(token.raw)}</span>`;
  }
  if (formulas.size >= 128) formulas.delete(formulas.keys().next().value);
  formulas.set(key, html);
  return html;
}

const markdown = new Marked({
  // Treat raw model-supplied HTML as text. KaTeX HTML is emitted only by the
  // trusted math renderer, with links/HTML commands explicitly disabled.
  renderer: { html: text => escape(text) },
  extensions: [
    {
      name: 'mathBlock', level: 'block',
      start: source => source.match(/^ {0,3}(?:\$\$|\\\[)/m)?.index,
      tokenizer(source) {
        const leading = source.match(/^ {0,3}(?=\$\$|\\\[)/)?.[0];
        if (leading === undefined) return;
        const token = readMath(source.slice(leading.length));
        if (!token?.display) return;
        const tail = source.slice(leading.length + token.raw.length).match(/^[ \t]*(?:\n|$)/);
        if (!tail) return;
        return { ...token, type: 'mathBlock', raw: leading + token.raw + tail[0] };
      },
      renderer: token => `<div class="math-display">${renderMath(token)}</div>\n`,
    },
    {
      name: 'mathInline', level: 'inline',
      start: source => source.search(/\$|\\[([]/),
      tokenizer(source) {
        const token = readMath(source);
        return token && { ...token, type: 'mathInline' };
      },
      renderer: token => token.display
        ? `<span class="math-display">${renderMath(token)}</span>`
        : renderMath(token),
    },
  ],
});

export const renderMarkdown = text => markdown.parse(text || '');
