/* 应用入口:初始化 + 哈希路由 */
(function () {
  const App = {
    state: { config: null, meta: null, health: null, stats: null, resumeId: null },
  };
  window.App = App;

  App.refreshBadge = function () {
    const badge = UI.$('#llm-badge');
    const config = App.state.config;
    const health = App.state.health;
    if (!badge) return;
    if (config && config.llmConfigured) {
      badge.className = 'badge badge-ok';
      badge.textContent = '大模型:' + config.model;
    } else {
      badge.className = 'badge badge-danger';
      badge.textContent = '大模型未配置';
    }
    const path = UI.$('#data-path');
    if (path && health) path.textContent = health.dataFile || '';
  };

  function currentRoute() {
    const hash = location.hash || '#/';
    const parts = hash.replace(/^#\/?/, '').split('/').filter((p) => p !== '');
    return { name: parts[0] || 'home', arg: parts[1] };
  }

  function navigate() {
    const { name, arg } = currentRoute();
    const pages = {
      home: () => Pages.home(),
      interview: () => Pages.interview(),
      review: () => Pages.review(Number(arg)),
      history: () => Pages.history(),
      resume: () => Pages.resume(),
      salary: () => Pages.salary(),
      plan: () => Pages.plan(),
      settings: () => Pages.settings(),
    };
    UI.$$('.nav a').forEach((link) => {
      link.classList.toggle('active', link.dataset.nav === name);
    });
    const render = pages[name] || pages.home;
    Promise.resolve()
      .then(render)
      .catch((err) => {
        console.error(err);
        UI.toast('页面加载失败:' + (err && err.message ? err.message : err), 'error');
      });
  }

  async function boot() {
    const view = UI.$('#view');
    if (view) view.innerHTML = '<div class="card"><div class="empty">正在启动本地服务…</div></div>';
    try {
      const [health, meta, config] = await Promise.all([Api.health(), Api.meta(), Api.getConfig()]);
      App.state.health = health;
      App.state.meta = meta;
      App.state.config = config;
      App.refreshBadge();
      if (!config.llmConfigured) {
        UI.toast('还没有配置大模型,请到「设置」里填写接口地址和 API Key', 'error');
      }
    } catch (err) {
      console.error(err);
      const message = err && err.message ? err.message : '无法连接本地服务';
      if (view) view.innerHTML = `<div class="card"><h2>启动失败</h2><div class="muted">${UI.escapeHtml(message)}</div></div>`;
      return;
    }
    navigate();
  }

  window.addEventListener('hashchange', navigate);
  document.addEventListener('DOMContentLoaded', boot);
  if (document.readyState !== 'loading') boot();
})();
