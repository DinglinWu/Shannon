// Fixed, local vector artwork. Never interpolate names or model output into SVG.
const paths = {
  trash: '<path d="M6 9h20M12 9V5h8v4M9 9l1 18h12l1-18M14 14v8M18 14v8"/>',
  chats: '<path d="M15.7 14.8c-1 .5-2.2.8-3.5.8l-2.4-.2-3.5 1.8.8-2.9C5.2 13 4 11.2 4 9.3 4 5.8 7.6 3 12 3s8 2.8 8 6.3c0 .5-.1 1-.2 1.4"/><path d="M28 19c0 3.2-3 5.7-6.7 5.7l-2-.2-3 1.5.7-2.5c-1.5-1-2.4-2.6-2.4-4.5 0-3.1 3-5.7 6.7-5.7S28 15.9 28 19Z"/><circle class="icon-dot" cx="9" cy="8.7" r=".9"/><circle class="icon-dot" cx="15" cy="8.7" r=".9"/><circle class="icon-dot" cx="18.7" cy="18.5" r=".8"/><circle class="icon-dot" cx="23.7" cy="18.5" r=".8"/>',
  contacts: '<circle class="icon-fill" cx="12" cy="9" r="5"/><path class="icon-fill" d="M12 17c-5 0-9 3-9 8v2h18v-2c0-5-4-8-9-8Z"/><path d="M23 9h6M24 15h5M25 21h4"/>',
  discover: '<circle cx="16" cy="16" r="12"/><path class="icon-fill" d="m22 10-4 9-8 3 4-9Z"/><path d="m14 13 4 6"/>',
  me: '<path class="icon-fill" d="M16 3.5c-3.5 0-5.3 2.5-5.3 5.8 0 2.4.8 4.3 2.3 5.6v2.6c-2 2-6.6 2.8-8 4.9-.9 1.3-1.3 3.1-1.3 5h24.6c0-1.9-.4-3.7-1.3-5-1.4-2.1-6-2.9-8-4.9v-2.6c1.5-1.3 2.3-3.2 2.3-5.6 0-3.3-1.8-5.8-5.3-5.8Z"/>',
  add: '<circle cx="16" cy="16" r="12"/><path d="M16 9v14M9 16h14"/>',
  back: '<path d="m20 5-11 11 11 11"/>',
  close: '<path d="m9 9 14 14M23 9 9 23"/>',
  more: '<circle class="icon-dot" cx="6" cy="16" r="1.8"/><circle class="icon-dot" cx="16" cy="16" r="1.8"/><circle class="icon-dot" cx="26" cy="16" r="1.8"/>',
  chevron: '<path d="m12 8 8 8-8 8"/>',
  camera: '<path d="M4 10h6l2-4h8l2 4h6v16H4Z"/><circle cx="16" cy="17.5" r="5"/>',
  moments: '<g stroke="none"><path fill="#54b670" d="m16 2 8 5-8 6-7-8Z"/><path fill="#3eab9a" d="m24 7 6 8-10 1-4-3Z"/><path fill="#65aee1" d="m30 15-3 10-8-7 1-2Z"/><path fill="#727dc7" d="m27 25-10 5-1-11 3-1Z"/><path fill="#c77bb5" d="M17 30 7 26l7-8 2 1Z"/><path fill="#e9a66b" d="m7 26-5-9 11-1 1 2Z"/><path fill="#e9c45d" d="M2 17 7 7l7 7-1 2Z"/><path fill="#99c666" d="m7 7 9-5-2 12Z"/><circle fill="#fff" cx="16" cy="16" r="3"/></g>',
};
export function icon(name) {
  return `<svg class="wechat-icon icon-${name}" viewBox="0 0 32 32" aria-hidden="true" focusable="false">${paths[name] || paths.me}</svg>`;
}
