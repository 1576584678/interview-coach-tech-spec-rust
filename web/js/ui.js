/* 通用 UI 工具:DOM、Toast、Modal、SVG 图表、轻量 Markdown */
(function (global) {
  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

  function escapeHtml(text) {
    return String(text == null ? '' : text)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  function toast(message, type = '') {
    const host = $('#toast-host');
    const el = document.createElement('div');
    el.className = 'toast ' + (type === 'error' ? 'err' : type === 'success' ? 'ok' : '');
    el.textContent = message;
    host.appendChild(el);
    setTimeout(() => { el.style.opacity = '0'; el.style.transition = 'opacity .3s'; }, 3200);
    setTimeout(() => el.remove(), 3600);
  }

  function openModal(html, onMount) {
    const host = $('#modal-host');
    host.innerHTML = `<div class="modal-mask"><div class="modal">${html}</div></div>`;
    const mask = $('.modal-mask', host);
    mask.addEventListener('click', (event) => { if (event.target === mask) closeModal(); });
    if (onMount) onMount($('.modal', host));
  }

  function closeModal() {
    $('#modal-host').innerHTML = '';
  }

  function confirmDialog(title, message, onConfirm) {
    openModal(`
      <h3>${escapeHtml(title)}</h3>
      <p class="muted">${escapeHtml(message)}</p>
      <div class="row" style="justify-content:flex-end;margin-top:16px">
        <button class="btn shrink" data-cancel>取消</button>
        <button class="btn btn-primary shrink" data-ok>确定</button>
      </div>`, (modal) => {
      $('[data-cancel]', modal).onclick = closeModal;
      $('[data-ok]', modal).onclick = () => { closeModal(); onConfirm(); };
    });
  }

  function download(filename, content, mime = 'text/markdown;charset=utf-8') {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
      toast('已复制到剪贴板', 'success');
    } catch (err) {
      toast('复制失败,请手动选择文本', 'error');
    }
  }

  function scoreColor(score) {
    if (score >= 85) return '#16a34a';
    if (score >= 70) return '#2f6bff';
    if (score >= 60) return '#d97706';
    return '#dc2626';
  }

  /* 雷达图:五维评分 */
  function radar(values, labels, size = 260) {
    const center = size / 2;
    const radius = size / 2 - 42;
    const count = values.length;
    const point = (index, ratio) => {
      const angle = (Math.PI * 2 * index) / count - Math.PI / 2;
      return [center + Math.cos(angle) * radius * ratio, center + Math.sin(angle) * radius * ratio];
    };
    let grid = '';
    [0.25, 0.5, 0.75, 1].forEach((ratio) => {
      const points = values.map((_, index) => point(index, ratio).map((n) => n.toFixed(1)).join(',')).join(' ');
      grid += `<polygon points="${points}" fill="none" stroke="#e5e7eb" />`;
    });
    values.forEach((_, index) => {
      const [x, y] = point(index, 1);
      grid += `<line x1="${center}" y1="${center}" x2="${x.toFixed(1)}" y2="${y.toFixed(1)}" stroke="#e5e7eb" />`;
    });
    const dataPoints = values
      .map((value, index) => point(index, Math.max(0.04, Math.min(1, value / 100))).map((n) => n.toFixed(1)).join(','))
      .join(' ');
    let labelsSvg = '';
    labels.forEach((label, index) => {
      const [x, y] = point(index, 1.22);
      labelsSvg += `<text x="${x.toFixed(1)}" y="${y.toFixed(1)}" font-size="11" fill="#6b7280" text-anchor="middle" dominant-baseline="middle">${escapeHtml(label)}</text>`;
      const [vx, vy] = point(index, 1.06);
      labelsSvg += `<text x="${vx.toFixed(1)}" y="${(vy + 10).toFixed(1)}" font-size="10" fill="#9aa2af" text-anchor="middle">${values[index]}</text>`;
    });
    return `<svg class="chart" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}">
      ${grid}
      <polygon points="${dataPoints}" fill="rgba(47,107,255,.18)" stroke="#2f6bff" stroke-width="1.6" />
      ${labelsSvg}
    </svg>`;
  }

  /* 折线图:分数趋势 */
  function lineChart(points, width = 640, height = 200) {
    if (!points.length) return '<div class="empty">还没有已完成并出分的面试</div>';
    const pad = { left: 34, right: 14, top: 16, bottom: 26 };
    const innerW = width - pad.left - pad.right;
    const innerH = height - pad.top - pad.bottom;
    const scores = points.map((p) => p.score);
    const max = Math.max(100, ...scores);
    const stepX = points.length > 1 ? innerW / (points.length - 1) : 0;
    const x = (index) => (points.length > 1 ? pad.left + index * stepX : pad.left + innerW / 2);
    const y = (score) => pad.top + innerH - (score / max) * innerH;
    let grid = '';
    [0, 25, 50, 75, 100].forEach((value) => {
      const gy = y(value);
      grid += `<line x1="${pad.left}" y1="${gy.toFixed(1)}" x2="${width - pad.right}" y2="${gy.toFixed(1)}" stroke="#eef0f4" />`;
      grid += `<text x="${pad.left - 6}" y="${(gy + 3).toFixed(1)}" font-size="10" fill="#9aa2af" text-anchor="end">${value}</text>`;
    });
    const line = points.map((p, index) => `${x(index).toFixed(1)},${y(p.score).toFixed(1)}`).join(' ');
    let dots = '';
    points.forEach((p, index) => {
      dots += `<circle cx="${x(index).toFixed(1)}" cy="${y(p.score).toFixed(1)}" r="3.4" fill="#fff" stroke="${scoreColor(p.score)}" stroke-width="2"><title>${escapeHtml(p.position)} ${p.score}分 ${escapeHtml(p.completedAt || '')}</title></circle>`;
    });
    return `<svg class="chart" viewBox="0 0 ${width} ${height}">
      ${grid}
      <polyline points="${line}" fill="none" stroke="#2f6bff" stroke-width="2" />
      ${dots}
    </svg>`;
  }

  function scoreRing(score) {
    const color = scoreColor(score);
    const degree = Math.max(0, Math.min(100, score)) * 3.6;
    return `<div class="score-ring" style="background:conic-gradient(${color} ${degree}deg, #eceef2 ${degree}deg)">
      <div class="inner"><div style="text-align:center">
        <div class="num" style="color:${color}">${score}</div>
        <div class="cap">总分</div>
      </div></div>
    </div>`;
  }

  /* 轻量 Markdown 渲染(标题/加粗/列表/段落),避免引入外部依赖 */
  function markdown(text) {
    const lines = String(text || '').replace(/\r\n/g, '\n').split('\n');
    let html = '';
    let inList = false;
    const inline = (value) => escapeHtml(value)
      .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
      .replace(/`(.+?)`/g, '<code>$1</code>');
    lines.forEach((line) => {
      const trimmed = line.trim();
      if (/^#{1,6}\s+/.test(trimmed)) {
        if (inList) { html += '</ul>'; inList = false; }
        const level = Math.min(3, (trimmed.match(/^#+/) || ['#'])[0].length);
        html += `<h${level}>${inline(trimmed.replace(/^#+\s+/, ''))}</h${level}>`;
      } else if (/^[-*]\s+/.test(trimmed) || /^\d+\.\s+/.test(trimmed)) {
        if (!inList) { html += '<ul>'; inList = true; }
        html += `<li>${inline(trimmed.replace(/^([-*]|\d+\.)\s+/, ''))}</li>`;
      } else if (!trimmed) {
        if (inList) { html += '</ul>'; inList = false; }
      } else {
        if (inList) { html += '</ul>'; inList = false; }
        html += `<p>${inline(trimmed)}</p>`;
      }
    });
    if (inList) html += '</ul>';
    return `<div class="md">${html}</div>`;
  }

  global.UI = {
    $, $$, escapeHtml, toast, openModal, closeModal, confirmDialog,
    download, copy, radar, lineChart, scoreRing, markdown, scoreColor,
  };
})(window);
