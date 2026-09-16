/* 页面渲染:概览 / 模拟面试 / 复盘 / 历史 / 简历 / 薪资 / 提升计划 / 设置 */
(function (global) {
  const { $, escapeHtml, toast, openModal, closeModal, confirmDialog, download, copy, radar, lineChart, scoreRing, markdown } = UI;
  const Pages = {};
  const DIMENSION_LABEL = { professional: '专业技术', expression: '表达清晰', logic: '逻辑思维', communication: '沟通能力', stress: '抗压能力' };
  const TYPE_LABEL = { basic: '基础技术', project: '项目深挖', deep: '场景设计', open: '开放题', diagnosis: '简历追问' };
  const TYPE_COLOR = { basic: 'badge-brand', project: 'badge-ok', deep: 'badge-warn', open: 'badge-muted', diagnosis: 'badge-danger' };

  let session = null;      // 当前面试(进行中)
  let streamAbort = null;  // 流式请求中断用

  const view = () => $('#view');
  const setView = (html) => { view().innerHTML = html; };
  const setTitle = (text) => { $('#page-title').textContent = text; };
  const setActions = (html) => { $('#topbar-actions').innerHTML = html || ''; };

  function fail(err) {
    console.error(err);
    toast(err && err.message ? err.message : '操作失败,请重试', 'error');
  }

  function llmReady() {
    const config = App.state.config;
    if (config && config.llmConfigured) return true;
    toast('请先在「设置」里配置大模型', 'error');
    location.hash = '#/settings';
    return false;
  }

  function options(items, value, placeholder) {
    const head = placeholder ? `<option value="">${escapeHtml(placeholder)}</option>` : '';
    return head + (items || []).map((item) => {
      const selected = String(item.value) === String(value) ? ' selected' : '';
      return `<option value="${escapeHtml(item.value)}"${selected}>${escapeHtml(item.label)}</option>`;
    }).join('');
  }

  function statusBadge(status) {
    if (status === 'completed') return '<span class="badge badge-ok">已完成</span>';
    if (status === 'abandoned') return '<span class="badge badge-muted">已放弃</span>';
    return '<span class="badge badge-warn">进行中</span>';
  }

  function scoreChip(score) {
    if (score == null) return '<span class="muted">—</span>';
    return `<span style="font-weight:600;color:${UI.scoreColor(score)}">${score}</span>`;
  }

  function dimensionBars(scores) {
    return Object.keys(DIMENSION_LABEL).map((key) => {
      const value = (scores && scores[key]) || 0;
      const color = UI.scoreColor(value);
      return `<div style="margin-bottom:10px">
        <div style="display:flex;justify-content:space-between;font-size:12px;color:#6b7280">
          <span>${DIMENSION_LABEL[key]}</span><span style="color:${color};font-weight:600">${value}</span>
        </div>
        <div class="progress" style="margin-top:5px"><div style="width:${value}%;background:${color}"></div></div>
      </div>`;
    }).join('');
  }

  function bindGoto(root) {
    root.onclick = (event) => {
      const target = event.target.closest('[data-goto]');
      if (target) location.hash = target.dataset.goto;
    };
  }

  /* ===================== 概览 ===================== */
  Pages.home = async function () {
    setTitle('概览');
    setActions('<button class="btn btn-primary" data-goto="#/interview">开始模拟面试</button>');
    setView('<div class="card"><div class="empty">加载中…</div></div>');
    bindGoto($('#topbar-actions'));

    let stats;
    try {
      stats = await Api.stats();
    } catch (err) { fail(err); return; }
    App.state.stats = stats;

    const dimensions = stats.dimensionAverages || {};
    const dimensionValues = Object.keys(DIMENSION_LABEL).map((key) => dimensions[key] || 0);
    const dimensionLabels = Object.keys(DIMENSION_LABEL).map((key) => DIMENSION_LABEL[key]);

    const weakHtml = (stats.weakPoints || []).length
      ? stats.weakPoints.map((point) => `<div class="list-item" style="cursor:default">
          <div class="title">${escapeHtml(point.label)} <span class="badge ${TYPE_COLOR[point.questionType] || 'badge-muted'}">${escapeHtml(TYPE_LABEL[point.questionType] || point.questionType)}</span></div>
          <div class="sub">平均 ${point.averageScore} 分 · 共 ${point.count} 题${point.sampleQuestions && point.sampleQuestions.length ? ' · 例:' + escapeHtml(point.sampleQuestions[0]) : ''}</div>
        </div>`).join('')
      : '<div class="empty">暂无薄弱点,继续保持 👍(完成面试并生成复盘后统计)</div>';

    setView(`
      <div class="grid grid-4" style="margin-bottom:16px">
        <div class="card stat"><div class="label">面试总场次</div><div class="value">${stats.totalSessions}<small> 场</small></div></div>
        <div class="card stat"><div class="label">已完成</div><div class="value">${stats.completedSessions}<small> 场</small></div></div>
        <div class="card stat"><div class="label">平均分</div><div class="value" style="color:${stats.avgScore != null ? UI.scoreColor(stats.avgScore) : '#1f2329'}">${stats.avgScore != null ? stats.avgScore : '—'}</div></div>
        <div class="card stat"><div class="label">最高分</div><div class="value">${stats.maxScore != null ? stats.maxScore : '—'}</div></div>
      </div>
      <div class="grid grid-2">
        <div class="card">
          <h2>分数趋势<span class="sub">按完成时间</span></h2>
          ${lineChart(stats.trend || [])}
        </div>
        <div class="card">
          <h2>五维能力<span class="sub">最近 ${Math.min(10, stats.completedSessions || 0)} 场平均</span></h2>
          <div style="display:flex;justify-content:center">${radar(dimensionValues, dimensionLabels)}</div>
        </div>
      </div>
      <div class="grid grid-2">
        <div class="card">
          <h2>薄弱点<span class="sub">平均分低于 70 的题型</span></h2>
          ${weakHtml}
        </div>
        <div class="card">
          <h2>快捷入口</h2>
          <div class="row">
            <button class="btn btn-primary" data-goto="#/interview">🎤 模拟面试</button>
            <button class="btn" data-goto="#/resume">📄 简历优化</button>
            <button class="btn" data-goto="#/salary">💰 薪资定位</button>
            <button class="btn" data-goto="#/plan">🧭 提升计划</button>
          </div>
          <div class="divider"></div>
          <div class="tiny muted">
            数据全部保存在本地文件:${escapeHtml(App.state.health ? App.state.health.dataFile : '')}<br />
            大模型:${escapeHtml(App.state.config ? (App.state.config.llmConfigured ? App.state.config.model : '未配置') : '')}
          </div>
        </div>
      </div>`);
    bindGoto(view());
  };

  /* ===================== 模拟面试 ===================== */
  Pages.interview = async function () {
    setTitle('模拟面试');
    setActions('');
    session = null;
    setView('<div class="card"><div class="empty">加载中…</div></div>');

    let resumes = [];
    try { resumes = await Api.resumeList(); } catch (err) { resumes = []; }
    const meta = App.state.meta || {};
    const config = App.state.config || {};

    let ongoing = null;
    try {
      const history = await Api.history(1, 20);
      ongoing = (history.list || []).find((item) => item.status === 'ongoing');
    } catch (err) { /* ignore */ }

    const ongoingBanner = ongoing
      ? `<div class="card" style="border-color:#f3d9a4;background:#fffaf0">
          <h2>发现一场未完成的面试</h2>
          <div class="muted">岗位:${escapeHtml(ongoing.position)} · 开始于 ${escapeHtml(ongoing.startedAt)} · 已答 ${ongoing.answeredCount}/${ongoing.questionCount} 题</div>
          <div class="row" style="margin-top:12px">
            <button class="btn btn-primary shrink" id="resume-session">继续这场面试</button>
            <button class="btn shrink" id="discard-session">放弃这场面试</button>
          </div>
        </div>`
      : '';

    setView(`
      ${ongoingBanner}
      <div class="card">
        <h2>开始一场模拟面试<span class="sub">AI 面试官会连续追问 ${config.totalQuestions || 11} 题</span></h2>
        <div class="grid grid-3">
          <div class="field">
            <label>目标岗位 *</label>
            <input id="f-position" placeholder="例如:Java后端开发 / 数据分析师" />
          </div>
          <div class="field">
            <label>岗位类别</label>
            <select id="f-category">${options(meta.positionCategories, 'backend')}</select>
          </div>
          <div class="field">
            <label>难度</label>
            <select id="f-difficulty">${options(meta.difficulties, 'normal')}</select>
          </div>
          <div class="field">
            <label>面试官风格</label>
            <select id="f-style">${options(meta.styles, 'friendly')}</select>
          </div>
          <div class="field">
            <label>面试模式</label>
            <select id="f-mode">${options(meta.modes, 'normal')}</select>
          </div>
          <div class="field">
            <label>关联简历(可选)</label>
            <select id="f-resume">${options(resumes.map((r) => ({ value: r.resumeId, label: r.fileName })), '', '不使用简历')}</select>
          </div>
        </div>
        <div class="row" style="margin-top:6px">
          <button class="btn btn-primary shrink" id="start-btn">开始面试</button>
          <span class="tiny muted">提示:先做一次「简历诊断」再关联简历,面试官会优先追问高风险点。</span>
        </div>
      </div>`);

    const startBtn = $('#start-btn');
    startBtn.onclick = async () => {
      const position = $('#f-position').value.trim();
      if (!position) { toast('请填写目标岗位', 'error'); return; }
      if (!llmReady()) return;
      startBtn.disabled = true;
      startBtn.textContent = '正在生成第一题…';
      try {
        const data = await Api.startInterview({
          position,
          positionCategory: $('#f-category').value,
          difficulty: $('#f-difficulty').value,
          style: $('#f-style').value,
          mode: $('#f-mode').value,
          resumeId: $('#f-resume').value ? Number($('#f-resume').value) : null,
        });
        session = {
          id: data.sessionId,
          position: position,
          totalQuestions: data.totalQuestions,
          status: 'ongoing',
          qaList: [{ questionOrder: 1, question: data.firstQuestion, questionType: data.questionType, answer: null }],
        };
        renderSession();
      } catch (err) {
        fail(err);
        startBtn.disabled = false;
        startBtn.textContent = '开始面试';
      }
    };

    if (ongoing) {
      $('#resume-session').onclick = async () => {
        try {
          session = await Api.interviewDetail(ongoing.sessionId);
          renderSession();
        } catch (err) { fail(err); }
      };
      $('#discard-session').onclick = () => {
        confirmDialog('放弃这场面试?', '放弃后不会生成复盘报告。', async () => {
          try {
            await Api.abandonInterview(ongoing.sessionId);
            toast('已放弃', 'success');
            Pages.interview();
          } catch (err) { fail(err); }
        });
      };
    }
  };

  function renderSession() {
    if (!session) return;
    setTitle(`模拟面试 · ${session.position}`);
    setActions('<button class="btn btn-sm" id="abandon-btn">放弃面试</button>');
    const qaList = session.qaList || [];
    const answered = qaList.filter((qa) => qa.answer).length;
    const total = session.totalQuestions || qaList.length;
    const current = qaList[qaList.length - 1];
    const finished = session.status && session.status !== 'ongoing';
    const awaitingComplete = !finished && current && current.answer;

    const bubbles = qaList.map((qa) => {
      const typeBadge = `<span class="badge ${TYPE_COLOR[qa.questionType] || 'badge-muted'}">${escapeHtml(TYPE_LABEL[qa.questionType] || qa.questionType || '')}</span>`;
      let html = `<div class="msg">
        <div class="avatar">🧑‍💼</div>
        <div class="bubble"><div class="meta">第 ${qa.questionOrder} 题 ${typeBadge}</div><div>${escapeHtml(qa.question)}</div></div>
      </div>`;
      if (qa.answer) {
        html += `<div class="msg me">
          <div class="avatar">🙋</div>
          <div class="bubble"><div class="meta">我的回答</div><div>${escapeHtml(qa.answer)}</div></div>
        </div>`;
      }
      return html;
    }).join('');

    const answerPanel = finished
      ? `<div class="card">
          <h2>本场面试已结束</h2>
          <div class="row"><button class="btn btn-primary shrink" id="view-review">查看复盘报告</button></div>
        </div>`
      : awaitingComplete
        ? `<div class="card">
            <h2>所有题目都答完了 🎉</h2>
            <div class="row"><button class="btn btn-primary shrink" id="complete-btn">结束面试并生成复盘</button></div>
          </div>`
        : `<div class="card">
            <h2>我的回答<span class="sub">第 ${current ? current.questionOrder : 1} 题</span></h2>
            <textarea id="answer-input" placeholder="像真实面试一样口述式作答,越具体越好(建议 100-400 字)"></textarea>
            <div class="row" style="margin-top:12px">
              <button class="btn btn-primary shrink" id="submit-btn">提交回答</button>
              <button class="btn shrink" id="skip-btn">跳过这题</button>
              <span class="tiny muted" id="stream-hint"></span>
            </div>
          </div>`;

    setView(`
      <div class="card">
        <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
          <div class="tiny muted">进度:第 ${Math.min(qaList.length, total)} / ${total} 题 · 已作答 ${answered} 题</div>
          <div class="tiny muted">${finished ? '本场已结束' : '回答后 AI 会立刻追问下一题'}</div>
        </div>
        <div class="progress"><div style="width:${Math.round((answered / total) * 100)}%"></div></div>
      </div>
      <div class="card">
        <div class="chat scroll" id="chat">${bubbles}</div>
      </div>
      ${answerPanel}
      ${finished ? '' : `<div class="card">
        <h2>追问预览</h2>
        <div class="tiny" id="next-question">等待生成…</div>
      </div>`}`);

    const chat = $('#chat');
    if (chat) chat.scrollTop = chat.scrollHeight;

    if ($('#abandon-btn')) {
      $('#abandon-btn').onclick = () => confirmDialog('放弃这场面试?', '本场不会生成复盘报告。', async () => {
        try {
          await Api.abandonInterview(session.id);
          toast('已放弃本场面试', 'success');
          session = null;
          Pages.interview();
        } catch (err) { fail(err); }
      });
    }
    if ($('#view-review')) {
      $('#view-review').onclick = () => { location.hash = `#/review/${session.id}`; };
    }
    if ($('#complete-btn')) {
      $('#complete-btn').onclick = async () => {
        const button = $('#complete-btn');
        button.disabled = true;
        button.textContent = '正在生成复盘…';
        try {
          await Api.completeInterview(session.id);
          location.hash = `#/review/${session.id}`;
        } catch (err) {
          fail(err);
          button.disabled = false;
          button.textContent = '结束面试并生成复盘';
        }
      };
    }
    if ($('#skip-btn')) {
      $('#skip-btn').onclick = async () => {
        if (!llmReady()) return;
        $('#skip-btn').disabled = true;
        $('#submit-btn').disabled = true;
        $('#stream-hint').textContent = '正在生成下一题…';
        try {
          const response = await Api.skipQuestion(session.id);
          applyAnswerResult('(候选人跳过了这道题)', response);
        } catch (err) { fail(err); renderSession(); }
      };
    }
    if ($('#submit-btn')) {
      $('#submit-btn').onclick = async () => {
        const answer = $('#answer-input').value.trim();
        if (answer.length < 2) { toast('请先写下你的回答', 'error'); return; }
        if (!llmReady()) return;
        $('#submit-btn').disabled = true;
        $('#skip-btn').disabled = true;
        $('#stream-hint').textContent = '面试官正在思考下一个问题…';
        const preview = $('#next-question');
        preview.textContent = '';
        let buffered = '';
        streamAbort = new AbortController();
        try {
          await Api.answerInterviewStream(session.id, answer, {
            signal: streamAbort.signal,
            onDelta: (text) => { buffered += text; preview.textContent = buffered; },
            onDone: (response) => { applyAnswerResult(answer, response); },
            onError: (payload) => { fail(payload); renderSession(); },
          });
        } catch (err) {
          if (err.name === 'AbortError') return;
          fail(err);
          renderSession();
        } finally {
          streamAbort = null;
        }
      };
    }
  }

  function applyAnswerResult(answer, response) {
    const qaList = session.qaList || [];
    const current = qaList[qaList.length - 1];
    if (current && !current.answer) current.answer = answer;
    if (response && response.nextQuestion) {
      qaList.push({
        questionOrder: response.questionOrder,
        question: response.nextQuestion,
        questionType: response.questionType,
        answer: null,
      });
    }
    renderSession();
  }

  /* ===================== 复盘报告 ===================== */
  Pages.review = async function (sessionId) {
    setTitle('复盘报告');
    setActions('<button class="btn btn-sm" data-goto="#/history">返回历史</button>');
    bindGoto($('#topbar-actions'));
    setView('<div class="card"><div class="empty">正在加载复盘报告…</div></div>');

    let detail;
    try {
      detail = await Api.interviewResult(sessionId);
    } catch (err) { fail(err); return; }

    const pending = detail.status === 'completed' && !detail.review && detail.reviewStatus !== 'failed';
    if (pending) {
      setView(`<div class="card">
        <h2>复盘生成中…</h2>
        <div class="muted">AI 评委正在逐题评分,通常需要 20-60 秒,请稍候。</div>
        <div class="progress" style="margin-top:12px"><div style="width:66%"></div></div>
        <div class="tiny muted" style="margin-top:10px">页面会自动刷新,不用手动重试。</div>
      </div>`);
      setTimeout(() => { if (location.hash === `#/review/${sessionId}`) Pages.review(sessionId); }, 3000);
      return;
    }
    if (!detail.review) {
      setView(`<div class="card">
        <h2>复盘生成失败</h2>
        <div class="muted">${escapeHtml(detail.reviewError || '未知原因')}</div>
        <div class="row" style="margin-top:12px">
          <button class="btn btn-primary shrink" id="retry-btn">重新生成复盘</button>
        </div>
      </div>`);
      $('#retry-btn').onclick = async () => {
        try {
          await Api.retryReview(sessionId);
          Pages.review(sessionId);
        } catch (err) { fail(err); }
      };
      return;
    }

    const review = detail.review;
    const dimensions = review.dimensionScores || {};
    const values = Object.keys(DIMENSION_LABEL).map((key) => dimensions[key] || 0);
    const labels = Object.keys(DIMENSION_LABEL).map((key) => DIMENSION_LABEL[key]);
    const feedbackByOrder = {};
    (review.qaFeedback || []).forEach((fb) => { feedbackByOrder[fb.questionOrder] = fb; });

    const qaItems = (detail.qaList || []).map((qa) => {
      const fb = feedbackByOrder[qa.questionOrder] || {};
      const score = fb.score != null ? fb.score : qa.score;
      return `<div class="qa-item">
        <div class="qa-head" data-toggle>
          <span class="badge ${TYPE_COLOR[qa.questionType] || 'badge-muted'}">${escapeHtml(TYPE_LABEL[qa.questionType] || qa.questionType || '')}</span>
          <span class="q">第 ${qa.questionOrder} 题 · ${escapeHtml((qa.question || '').slice(0, 60))}</span>
          ${scoreChip(score)}
        </div>
        <div class="qa-body">
          <div class="block"><div class="k">问题</div><div>${escapeHtml(qa.question)}</div></div>
          <div class="block"><div class="k">我的回答</div><div>${escapeHtml(qa.answer || '(未作答)')}</div></div>
          <div class="block"><div class="k">点评</div><div>${escapeHtml(fb.feedback || qa.feedback || '—')}</div></div>
          <div class="block"><div class="k">更好的回答</div><div>${escapeHtml(fb.betterAnswer || qa.betterAnswer || '—')}</div></div>
        </div>
      </div>`;
    }).join('');

    setView(`
      <div class="card">
        <div class="score-hero">
          ${scoreRing(review.overallScore)}
          <div style="min-width:260px;flex:1">
            <div style="font-size:16px;font-weight:600">${escapeHtml(detail.position)}</div>
            <div class="tiny muted" style="margin-bottom:10px">
              ${escapeHtml(detail.startedAt || '')} · 难度 ${escapeHtml(detail.difficulty || '')} · ${(detail.qaList || []).length} 题
            </div>
            ${dimensionBars(dimensions)}
          </div>
          <div style="display:flex;justify-content:center">${radar(values, labels)}</div>
        </div>
        <div class="divider"></div>
        <div class="row">
          <button class="btn shrink" id="export-btn">导出 Markdown</button>
          <button class="btn shrink" id="copy-btn">复制报告</button>
          <button class="btn shrink" id="again-btn">再面一场</button>
        </div>
      </div>
      <div class="grid grid-2">
        <div class="card"><h2>亮点</h2><div class="pre">${escapeHtml(review.strengths || '—')}</div></div>
        <div class="card"><h2>薄弱项</h2><div class="pre">${escapeHtml(review.weaknesses || '—')}</div></div>
      </div>
      <div class="grid grid-2">
        <div class="card"><h2>改进建议</h2><div class="pre">${escapeHtml(review.improvementPlan || '—')}</div></div>
        <div class="card"><h2>减分行为提醒</h2><div class="pre">${escapeHtml(review.riskWarnings || '—')}</div></div>
      </div>
      <div class="card">
        <h2>逐题复盘<span class="sub">点击展开</span></h2>
        ${qaItems || '<div class="empty">暂无记录</div>'}
      </div>`);

    view().addEventListener('click', (event) => {
      const head = event.target.closest('[data-toggle]');
      if (head) head.parentElement.classList.toggle('open');
    });
    const markdownText = buildReportMarkdown(detail, review);
    $('#export-btn').onclick = () => download(`面试复盘-${detail.position}-${detail.id}.md`, markdownText);
    $('#copy-btn').onclick = () => copy(markdownText);
    $('#again-btn').onclick = () => { location.hash = '#/interview'; };
  };

  function buildReportMarkdown(detail, review) {
    const lines = [];
    lines.push('# 面试复盘报告');
    lines.push('');
    lines.push('- 岗位:' + detail.position);
    lines.push('- 时间:' + (detail.startedAt || ''));
    lines.push('- 难度:' + (detail.difficulty || ''));
    lines.push('- 总分:**' + review.overallScore + '**');
    lines.push('');
    lines.push('## 维度得分');
    Object.keys(DIMENSION_LABEL).forEach((key) => {
      lines.push('- ' + DIMENSION_LABEL[key] + ':' + ((review.dimensionScores || {})[key] || 0));
    });
    lines.push('');
    lines.push('## 亮点');
    lines.push(review.strengths || '—');
    lines.push('');
    lines.push('## 薄弱项');
    lines.push(review.weaknesses || '—');
    lines.push('');
    lines.push('## 改进建议');
    lines.push(review.improvementPlan || '—');
    lines.push('');
    lines.push('## 减分行为提醒');
    lines.push(review.riskWarnings || '—');
    lines.push('');
    lines.push('## 逐题复盘');
    const feedbackByOrder = {};
    (review.qaFeedback || []).forEach((fb) => { feedbackByOrder[fb.questionOrder] = fb; });
    (detail.qaList || []).forEach((qa) => {
      const fb = feedbackByOrder[qa.questionOrder] || {};
      lines.push('');
      lines.push('### 第 ' + qa.questionOrder + ' 题(' + (TYPE_LABEL[qa.questionType] || qa.questionType) + ') 得分 ' + (fb.score != null ? fb.score : '—'));
      lines.push('**问题**:' + qa.question);
      lines.push('');
      lines.push('**我的回答**:' + (qa.answer || '(未作答)'));
      lines.push('');
      lines.push('**点评**:' + (fb.feedback || qa.feedback || '—'));
      lines.push('');
      lines.push('**更好的回答**:' + (fb.betterAnswer || qa.betterAnswer || '—'));
    });
    return lines.join('\n');
  }

  /* ===================== 历史记录 ===================== */
  Pages.history = async function () {
    setTitle('历史记录');
    setActions('');
    let page = 1;
    const size = 10;

    async function render() {
      setView('<div class="card"><div class="empty">加载中…</div></div>');
      let data;
      try {
        data = await Api.history(page, size);
      } catch (err) { fail(err); return; }
      const rows = (data.list || []).map((item) => `<tr>
        <td>${escapeHtml(item.startedAt || '')}</td>
        <td>${escapeHtml(item.position)}<div class="tiny muted">${escapeHtml(item.difficulty || '')}</div></td>
        <td>${statusBadge(item.status)}</td>
        <td>${scoreChip(item.totalScore)}</td>
        <td class="tiny muted">${item.answeredCount}/${item.questionCount} 题</td>
        <td>
          <button class="btn btn-sm" data-detail="${item.sessionId}">详情</button>
          ${item.status === 'completed' ? `<button class="btn btn-sm" data-review="${item.sessionId}">复盘</button>` : ''}
          <button class="btn btn-sm btn-danger" data-del="${item.sessionId}">删除</button>
        </td>
      </tr>`).join('');
      const pages = Math.max(1, Math.ceil((data.total || 0) / size));
      setView(`<div class="card">
        <h2>共 ${data.total || 0} 场面试</h2>
        <table>
          <thead><tr><th>时间</th><th>岗位</th><th>状态</th><th>总分</th><th>进度</th><th>操作</th></tr></thead>
          <tbody>${rows || '<tr><td colspan="6" class="empty">还没有面试记录</td></tr>'}</tbody>
        </table>
        <div class="row" style="margin-top:12px;align-items:center">
          <button class="btn btn-sm shrink" id="prev" ${page <= 1 ? 'disabled' : ''}>上一页</button>
          <span class="tiny muted">第 ${page} / ${pages} 页</span>
          <button class="btn btn-sm shrink" id="next" ${page >= pages ? 'disabled' : ''}>下一页</button>
        </div>
      </div>`);
      $('#prev').onclick = () => { if (page > 1) { page -= 1; render(); } };
      $('#next').onclick = () => { if (page < pages) { page += 1; render(); } };
      view().onclick = async (event) => {
        const id = event.target.dataset || {};
        if (id.detail) {
          try {
            const detail = await Api.interviewDetail(id.detail);
            openModal(`<h3>${escapeHtml(detail.position)} · ${escapeHtml(detail.startedAt || '')}</h3>
              <div class="scroll">${(detail.qaList || []).map((qa) => `<div class="list-item" style="cursor:default">
                  <div class="title">第 ${qa.questionOrder} 题 <span class="badge ${TYPE_COLOR[qa.questionType] || 'badge-muted'}">${escapeHtml(TYPE_LABEL[qa.questionType] || '')}</span></div>
                  <div class="sub">${escapeHtml(qa.question)}</div>
                  <div class="pre" style="margin-top:8px;max-height:none">${escapeHtml(qa.answer || '(未作答)')}</div>
                </div>`).join('')}</div>
              <div class="row" style="justify-content:flex-end;margin-top:12px"><button class="btn shrink" id="close-modal">关闭</button></div>`, (modal) => {
              $('#close-modal', modal).onclick = closeModal;
            });
          } catch (err) { fail(err); }
        } else if (id.review) {
          location.hash = '#/review/' + id.review;
        } else if (id.del) {
          confirmDialog('删除这场面试?', '删除后无法恢复(复盘报告会一起删除)。', async () => {
            try {
              await Api.deleteInterview(id.del);
              toast('已删除', 'success');
              render();
            } catch (err) { fail(err); }
          });
        }
      };
    }

    await render();
  };

  /* ===================== 简历优化 ===================== */
  Pages.resume = async function () {
    setTitle('简历优化');
    setActions('<button class="btn btn-sm btn-primary" id="new-resume">新建简历</button>');
    let currentId = App.state.resumeId || null;
    let list = [];
    let currentOptimized = '';

    async function loadList() {
      try {
        list = await Api.resumeList();
      } catch (err) { list = []; }
      if (!currentId && list.length) currentId = list[0].resumeId;
      App.state.resumeId = currentId;
    }

    async function render() {
      const items = list.length
        ? list.map((item) => `<div class="list-item ${item.resumeId === currentId ? 'active' : ''}" data-id="${item.resumeId}">
            <div class="title">${escapeHtml(item.fileName)}</div>
            <div class="sub">${escapeHtml(item.targetPosition || '未指定岗位')} · ${item.charCount} 字
              ${item.matchScore != null ? ' · 匹配度 ' + item.matchScore : ''}
              ${item.hasDiagnosis ? ' · 已诊断' : ''}${item.hasOptimization ? ' · 已优化' : ''}</div>
          </div>`).join('')
        : '<div class="empty">还没有简历<br />点击右上角「新建简历」上传或粘贴</div>';

      let detailHtml = '<div class="empty">选择左侧简历查看详情</div>';
      if (currentId) {
        let resume;
        try { resume = await Api.resumeDetail(currentId); } catch (err) { resume = null; }
        if (resume) {
          const diagnosis = resume.diagnosis;
          const optimization = resume.optimization;
          currentOptimized = optimization ? optimization.optimizedResume : '';
          detailHtml = `
            <div class="card">
              <h2>${escapeHtml(resume.fileName)}<span class="sub">${resume.rawText.length} 字 · ${escapeHtml(resume.createdAt)}</span></h2>
              <div class="grid grid-3">
                <div class="field">
                  <label>目标岗位</label>
                  <input id="r-position" value="${escapeHtml(resume.targetPosition || '')}" placeholder="例如:Java后端开发" />
                </div>
                <div class="field">
                  <label>简历风格</label>
                  <select id="r-style">${options((App.state.meta || {}).resumeStyles, 'general')}</select>
                </div>
                <div class="field">
                  <label>操作</label>
                  <div class="row">
                    <button class="btn btn-sm shrink" id="diag-btn">简历诊断</button>
                    <button class="btn btn-sm btn-primary shrink" id="opt-btn">AI 优化</button>
                    <button class="btn btn-sm btn-danger shrink" id="del-resume">删除</button>
                  </div>
                </div>
              </div>
              <div class="field">
                <label>岗位 JD(可选,粘贴后优化更精准)</label>
                <textarea id="r-jd" style="min-height:70px" placeholder="把招聘网站上的岗位描述粘贴到这里"></textarea>
              </div>
              <details>
                <summary class="tiny muted" style="cursor:pointer">查看简历原文</summary>
                <div class="pre" style="margin-top:8px">${escapeHtml(resume.rawText)}</div>
              </details>
            </div>
            ${diagnosis ? renderDiagnosis(diagnosis) : ''}
            ${optimization ? renderOptimization(optimization) : ''}
            <div class="card">
              <h2>项目经历 STAR 口述稿<span class="sub">面试可以直接照着说</span></h2>
              <div class="field">
                <label>项目原文(从简历里复制一段项目经历)</label>
                <textarea id="star-input" placeholder="例如:负责订单系统的重构,把下单链路从 800ms 优化到 200ms……"></textarea>
              </div>
              <div class="row"><button class="btn shrink" id="star-btn">生成 STAR 口述稿</button></div>
              <div id="star-result" style="margin-top:12px"></div>
            </div>`;
        }
      }

      setView(`<div style="display:flex;gap:16px;align-items:flex-start;flex-wrap:wrap">
        <div class="card" style="width:300px;flex:0 0 300px">
          <h2>我的简历<span class="sub">${list.length} 份</span></h2>
          ${items}
        </div>
        <div style="flex:1;min-width:420px">${detailHtml}</div>
      </div>`);

      view().onclick = async (event) => {
        const item = event.target.closest('.list-item[data-id]');
        if (item) {
          currentId = Number(item.dataset.id);
          App.state.resumeId = currentId;
          render();
        }
      };
      $('#new-resume').onclick = openNewResume;
      if (currentId) bindDetail();
    }

    let detailResumeId = null;

    function bindDetail() {
      detailResumeId = currentId;
      if ($('#r-position')) {
        $('#diag-btn').onclick = async () => {
          if (!llmReady()) return;
          const button = $('#diag-btn');
          button.disabled = true;
          button.textContent = '诊断中…';
          try {
            await Api.resumeDiagnose(currentId, {
              targetPosition: $('#r-position').value.trim(),
              jdText: $('#r-jd').value.trim(),
            });
            toast('诊断完成', 'success');
            await loadList();
            render();
          } catch (err) { fail(err); button.disabled = false; button.textContent = '简历诊断'; }
        };
        $('#opt-btn').onclick = async () => {
          if (!llmReady()) return;
          const button = $('#opt-btn');
          button.disabled = true;
          button.textContent = '优化中(约 30-90 秒)…';
          try {
            await Api.resumeOptimize(currentId, {
              targetPosition: $('#r-position').value.trim(),
              jdText: $('#r-jd').value.trim(),
              style: $('#r-style').value,
            });
            toast('优化完成', 'success');
            await loadList();
            render();
          } catch (err) { fail(err); button.disabled = false; button.textContent = 'AI 优化'; }
        };
        $('#del-resume').onclick = () => confirmDialog('删除这份简历?', '删除后无法恢复。', async () => {
          try {
            await Api.resumeDelete(currentId);
            currentId = null;
            App.state.resumeId = null;
            await loadList();
            render();
          } catch (err) { fail(err); }
        });
        $('#star-btn').onclick = async () => {
          if (!llmReady()) return;
          const text = $('#star-input').value.trim();
          if (!text) { toast('请先粘贴一段项目经历', 'error'); return; }
          const button = $('#star-btn');
          button.disabled = true;
          button.textContent = '生成中…';
          try {
            const result = await Api.resumeStar(currentId, {
              projectText: text,
              targetPosition: $('#r-position').value.trim(),
              jdText: $('#r-jd').value.trim(),
            });
            $('#star-result').innerHTML = markdown(result.markdown);
          } catch (err) { fail(err); } finally {
            button.disabled = false;
            button.textContent = '生成 STAR 口述稿';
          }
        };
        if ($('#copy-optimized')) {
          $('#copy-optimized').onclick = () => copy(currentOptimized);
        }
        if ($('#download-optimized')) {
          $('#download-optimized').onclick = () => {
            download('优化简历.txt', currentOptimized, 'text/plain;charset=utf-8');
          };
        }
      }
    }

    function renderDiagnosis(diagnosis) {
      const issues = (diagnosis.criticalIssues || []).map((issue) => `<div class="list-item" style="cursor:default">
          <div class="title">❗ ${escapeHtml(issue.title)}</div>
          <div class="sub">${escapeHtml(issue.detail)}</div>
          <div class="sub" style="color:#15803d">怎么改:${escapeHtml(issue.fix)}</div>
        </div>`).join('');
      const questions = (diagnosis.followUpQuestions || []).map((q) => `<li>${escapeHtml(q)}</li>`).join('');
      const missing = (diagnosis.missingSkills || []).map((s) => `<span class="badge badge-warn" style="margin:0 6px 6px 0">${escapeHtml(s)}</span>`).join('');
      return `<div class="card">
        <h2>简历诊断<span class="sub">匹配度 ${diagnosis.matchScore} · ${escapeHtml(diagnosis.summary || '')}</span></h2>
        ${issues || '<div class="muted">未发现致命问题</div>'}
        <div class="divider"></div>
        <div class="grid grid-2">
          <div><div class="tiny muted" style="margin-bottom:6px">缺失技能</div>${missing || '<span class="muted">—</span>'}</div>
          <div><div class="tiny muted" style="margin-bottom:6px">面试官最可能追问</div><ul class="tiny">${questions || '<li>—</li>'}</ul></div>
        </div>
      </div>`;
    }

    function renderOptimization(optimization) {
      const suggestions = (optimization.suggestions || []).map((item) => `<div class="list-item" style="cursor:default">
          <div class="title">${escapeHtml(item.section)} <span class="badge ${item.impact === 'high' ? 'badge-danger' : item.impact === 'low' ? 'badge-muted' : 'badge-warn'}">${escapeHtml(item.impact || 'medium')}</span></div>
          <div class="sub">原文:${escapeHtml(item.original)}</div>
          <div class="sub" style="color:#15803d">改为:${escapeHtml(item.optimized)}</div>
          <div class="sub">理由:${escapeHtml(item.reason)}</div>
        </div>`).join('');
      const projects = (optimization.projects || []).map((item) => `<div class="list-item" style="cursor:default">
          <div class="title">${escapeHtml(item.name)}</div>
          <div class="sub">原文:${escapeHtml(item.original)}</div>
          <div class="sub" style="color:#2359e0">建议:${escapeHtml(item.suggestion)}</div>
        </div>`).join('');
      const risks = ((optimization.diagnostics || {}).risks || []).map((risk) => `<li>${escapeHtml(risk.section)}:${escapeHtml(risk.risk)} → ${escapeHtml(risk.suggestion)}</li>`).join('');
      const keywords = (optimization.keywords || []).map((k) => `<span class="badge badge-brand" style="margin:0 6px 6px 0">${escapeHtml(k)}</span>`).join('');
      return `<div class="card">
        <h2>AI 优化结果<span class="sub">匹配度 ${optimization.matchScore}</span></h2>
        <div class="muted" style="margin-bottom:10px">${escapeHtml(optimization.matchAnalysis || '')}</div>
        ${projects ? '<div class="tiny muted" style="margin:10px 0 6px">项目逐条优化</div>' + projects : ''}
        ${suggestions ? '<div class="tiny muted" style="margin:10px 0 6px">其他优化建议</div>' + suggestions : ''}
        ${keywords ? '<div class="tiny muted" style="margin:10px 0 6px">建议补充的关键词</div><div>' + keywords + '</div>' : ''}
        ${risks ? '<div class="tiny muted" style="margin:10px 0 6px">风险与补证</div><ul class="tiny">' + risks + '</ul>' : ''}
        <div class="divider"></div>
        <h2>优化后的完整简历</h2>
        <div class="pre">${escapeHtml(optimization.optimizedResume)}</div>
        <div class="row" style="margin-top:10px">
          <button class="btn btn-sm shrink" id="copy-optimized">复制全文</button>
          <button class="btn btn-sm shrink" id="download-optimized">下载 txt</button>
        </div>
      </div>`;
    }

    function openNewResume() {
      openModal(`<h3>新建简历</h3>
        <div class="row" style="margin-bottom:10px">
          <button class="btn btn-sm btn-primary shrink" id="tab-file">上传文件</button>
          <button class="btn btn-sm shrink" id="tab-text">粘贴文本</button>
        </div>
        <div id="pane-file">
          <div class="field"><label>简历文件(txt / md / docx / pdf,扫描件请改用粘贴)</label><input type="file" id="file-input" accept=".txt,.md,.docx,.pdf,.json,.csv,.html" /></div>
          <div class="field"><label>目标岗位(可选)</label><input id="upload-position" placeholder="例如:Java后端开发" /></div>
        </div>
        <div id="pane-text" style="display:none">
          <div class="field"><label>简历文本</label><textarea id="text-input" style="min-height:180px" placeholder="把简历内容粘贴到这里"></textarea></div>
          <div class="field"><label>目标岗位(可选)</label><input id="text-position" placeholder="例如:Java后端开发" /></div>
        </div>
        <div class="row" style="justify-content:flex-end;margin-top:12px">
          <button class="btn shrink" id="cancel-new">取消</button>
          <button class="btn btn-primary shrink" id="submit-new">保存</button>
        </div>`, (modal) => {
        const fileTab = $('#tab-file', modal);
        const textTab = $('#tab-text', modal);
        fileTab.onclick = () => {
          $('#pane-file', modal).style.display = '';
          $('#pane-text', modal).style.display = 'none';
          fileTab.classList.add('btn-primary');
          textTab.classList.remove('btn-primary');
        };
        textTab.onclick = () => {
          $('#pane-file', modal).style.display = 'none';
          $('#pane-text', modal).style.display = '';
          textTab.classList.add('btn-primary');
          fileTab.classList.remove('btn-primary');
        };
        fileTab.classList.add('btn-primary');
        $('#cancel-new', modal).onclick = closeModal;
        $('#submit-new', modal).onclick = async () => {
          const button = $('#submit-new', modal);
          button.disabled = true;
          try {
            let saved;
            const fileInput = $('#file-input', modal);
            if (fileInput.files && fileInput.files.length) {
              saved = await Api.resumeUpload(fileInput.files[0], $('#upload-position', modal).value.trim());
            } else {
              const text = $('#text-input', modal).value.trim();
              if (text.length < 20) { toast('请上传文件或粘贴至少 20 字的简历内容', 'error'); button.disabled = false; return; }
              saved = await Api.resumeFromText({ rawText: text, targetPosition: $('#text-position', modal).value.trim() });
            }
            closeModal();
            currentId = saved.resumeId;
            App.state.resumeId = currentId;
            await loadList();
            render();
            toast('简历已保存', 'success');
          } catch (err) {
            fail(err);
            button.disabled = false;
          }
        };
      });
    }

    await loadList();
    await render();
  };

  /* ===================== 薪资定位 ===================== */
  Pages.salary = async function () {
    setTitle('薪资定位');
    setActions('');
    setView(`<div class="card">
      <h2>岗位薪资范围估计<span class="sub">基于本地公开招聘基准数据(可离线计算)</span></h2>
      <div class="grid grid-3">
        <div class="field"><label>岗位 *</label><input id="s-position" placeholder="例如:Java后端开发 / 前端 / 产品经理" /></div>
        <div class="field"><label>城市</label><input id="s-city" placeholder="例如:杭州(留空按二线城市)" /></div>
        <div class="field"><label>经验</label><input id="s-experience" placeholder="例如:3年 / 应届 / 5年以上" /></div>
      </div>
      <div class="row"><button class="btn btn-primary shrink" id="s-btn">查询薪资范围</button></div>
    </div>
    <div id="s-result"></div>`);

    $('#s-btn').onclick = async () => {
      const position = $('#s-position').value.trim();
      if (!position) { toast('请填写岗位', 'error'); return; }
      const button = $('#s-btn');
      button.disabled = true;
      button.textContent = '查询中…';
      try {
        const range = await Api.salary(position, $('#s-city').value.trim(), $('#s-experience').value.trim());
        renderSalary(range);
      } catch (err) {
        fail(err);
      } finally {
        button.disabled = false;
        button.textContent = '查询薪资范围';
      }
    };

    function renderSalary(range) {
      const max = range.p75;
      const bar = (value, label, color) => `<div style="margin-bottom:12px">
        <div style="display:flex;justify-content:space-between;font-size:13px"><span>${label}</span><span style="font-weight:650;color:${color}">${value.toLocaleString()} 元/月</span></div>
        <div class="progress" style="margin-top:6px"><div style="width:${Math.round((value / max) * 100)}%;background:${color}"></div></div>
      </div>`;
      $('#s-result').innerHTML = `<div class="card">
        <h2>${escapeHtml(range.position)} · ${escapeHtml(range.city || '全国参考')}
          <span class="sub">${escapeHtml(range.cityTier)} · ${escapeHtml(range.experienceLevel)}</span>
          <span class="badge ${range.confidence === 'high' ? 'badge-ok' : range.confidence === 'medium' ? 'badge-warn' : 'badge-danger'}">${range.confidence === 'high' ? '基准表命中' : range.confidence === 'medium' ? '近似岗位匹配' : 'AI 估计'}</span>
        </h2>
        ${bar(range.p25, 'P25(偏低)', '#93b4ff')}
        ${bar(range.p50, 'P50(中位数)', '#2f6bff')}
        ${bar(range.p75, 'P75(偏高)', '#16a34a')}
        <div class="tiny muted" style="margin-top:8px">${escapeHtml(range.note || '')}</div>
      </div>`;
    }
  };

  /* ===================== 提升计划 ===================== */
  Pages.plan = async function () {
    setTitle('提升计划');
    setActions('<button class="btn btn-sm" id="regen-plan">重新生成</button>');
    setView('<div class="card"><div class="empty">加载中…</div></div>');
    $('#regen-plan').onclick = () => load(true);

    async function load(force) {
      setView('<div class="card"><div class="empty">' + (force ? '正在重新生成计划(约 20-60 秒)…' : '加载中…') + '</div></div>');
      try {
        const plan = force ? await Api.regeneratePlan() : await Api.improvementPlan();
        render(plan);
      } catch (err) {
        setView(`<div class="card">
          <h2>还没有可用的提升计划</h2>
          <div class="muted">${escapeHtml(err.message || '')}</div>
          <div class="row" style="margin-top:12px"><button class="btn btn-primary shrink" data-goto="#/interview">去做一场模拟面试</button></div>
        </div>`);
        bindGoto(view());
      }
    }

    function render(plan) {
      const focus = (plan.focusAreas || []).map((item) => `<div class="list-item" style="cursor:default">
          <div class="title">P${item.priority} · ${escapeHtml(item.name)}</div>
          <div class="sub">${escapeHtml(item.reason)}</div>
          <ul class="tiny" style="margin:6px 0 0">${(item.exercises || []).map((ex) => `<li>${escapeHtml(ex)}</li>`).join('')}</ul>
        </div>`).join('');
      const weeks = (plan.weeklyPlan || []).map((week) => `<div class="card">
          <h2>第 ${week.week} 周 · ${escapeHtml(week.theme)}</h2>
          <ul class="tiny">${(week.tasks || []).map((task) => `<li>${escapeHtml(task)}</li>`).join('')}</ul>
        </div>`).join('');
      setView(`
        <div class="card">
          <h2>下一步练什么<span class="sub">生成于 ${escapeHtml(plan.generatedAt || '')} · 基于 ${plan.basedOnSessions || 0} 场复盘</span></h2>
          <div class="muted">${escapeHtml(plan.expectedGains || '')}</div>
        </div>
        <div class="card"><h2>重点提升项</h2>${focus || '<div class="empty">暂无</div>'}</div>
        <div class="grid grid-2">${weeks}</div>`);
    }

    await load(false);
  };

  /* ===================== 设置 ===================== */
  Pages.settings = async function () {
    setTitle('设置');
    setActions('');
    let config;
    try {
      config = await Api.getConfig();
    } catch (err) { fail(err); return; }
    App.state.config = config;
    App.refreshBadge();
    const meta = App.state.meta || {};

    setView(`
      <div class="card">
        <h2>大模型配置<span class="sub">支持任何 OpenAI 兼容接口(DeepSeek / 通义 / Kimi / 本地 Ollama 等)</span></h2>
        <div class="grid grid-2">
          <div class="field">
            <label>接口地址 Base URL</label>
            <input id="c-base" value="${escapeHtml(config.baseUrl)}" placeholder="https://api.deepseek.com" />
            <div class="hint">实际请求:${escapeHtml(config.chatCompletionsUrl || '')}</div>
          </div>
          <div class="field">
            <label>API Key</label>
            <input id="c-key" type="password" placeholder="${config.apiKeySet ? '已保存:' + escapeHtml(config.apiKeyMasked) + '(留空则不修改)' : 'sk-...'}" />
            <div class="hint">仅保存在本机 config.toml 文件里</div>
          </div>
          <div class="field">
            <label>模型名</label>
            <input id="c-model" value="${escapeHtml(config.model)}" placeholder="deepseek-chat" />
          </div>
          <div class="field">
            <label>超时(秒)</label>
            <input id="c-timeout" type="number" min="5" max="600" value="${config.timeoutSeconds}" />
          </div>
          <div class="field">
            <label>temperature(0-2)</label>
            <input id="c-temp" type="number" step="0.1" min="0" max="2" value="${config.temperature}" />
          </div>
          <div class="field">
            <label>max_tokens</label>
            <input id="c-maxtokens" type="number" min="1" max="32000" value="${config.maxTokens}" />
          </div>
          <div class="field">
            <label>每场面试题数</label>
            <input id="c-total" type="number" min="1" max="30" value="${config.totalQuestions}" />
          </div>
          <div class="field">
            <label>启动时自动打开浏览器</label>
            <select id="c-autoopen">
              <option value="true"${config.autoOpenBrowser ? ' selected' : ''}>开启</option>
              <option value="false"${config.autoOpenBrowser ? '' : ' selected'}>关闭</option>
            </select>
          </div>
        </div>
        <div class="row" style="margin-top:6px">
          <button class="btn btn-primary shrink" id="save-config">保存配置</button>
          <button class="btn shrink" id="test-config">测试连接</button>
          <span class="tiny muted" id="test-result"></span>
        </div>
      </div>
      <div class="grid grid-2">
        <div class="card">
          <h2>本机信息</h2>
          <table>
            <tr><td>版本</td><td>${escapeHtml(meta.version || '')}</td></tr>
            <tr><td>监听地址</td><td>${escapeHtml(config.host + ':' + config.port)}</td></tr>
            <tr><td>配置文件</td><td class="tiny">${escapeHtml(config.configFile)}</td></tr>
            <tr><td>数据目录</td><td class="tiny">${escapeHtml(config.dataDir)}</td></tr>
            <tr><td>题库</td><td>${(meta.questionBank || {}).total || 0} 道</td></tr>
            <tr><td>薪资基准</td><td>${meta.salaryBenchmarkCount || 0} 条</td></tr>
          </table>
        </div>
        <div class="card">
          <h2>使用说明</h2>
          <ul class="tiny muted">
            <li>所有数据只保存在本机,不会上传到第三方;仅面试/复盘文本会发给所配置的大模型接口。</li>
            <li>备份:直接拷贝数据目录下的 json 文件即可迁移到另一台电脑。</li>
            <li>简历支持 txt / md / docx / pdf(文本型);扫描件请改用「粘贴文本」。</li>
            <li>服务默认只监听 127.0.0.1,其他设备访问不到,更安全。</li>
          </ul>
        </div>
      </div>`);

    $('#save-config').onclick = async () => {
      const button = $('#save-config');
      button.disabled = true;
      const payload = {
        baseUrl: $('#c-base').value.trim(),
        model: $('#c-model').value.trim(),
        temperature: Number($('#c-temp').value),
        maxTokens: Number($('#c-maxtokens').value),
        timeoutSeconds: Number($('#c-timeout').value),
        totalQuestions: Number($('#c-total').value),
        autoOpenBrowser: $('#c-autoopen').value === 'true',
      };
      const key = $('#c-key').value.trim();
      if (key) payload.apiKey = key;
      try {
        const saved = await Api.updateConfig(payload);
        App.state.config = saved;
        App.refreshBadge();
        toast('配置已保存', 'success');
        Pages.settings();
      } catch (err) {
        fail(err);
        button.disabled = false;
      }
    };
    $('#test-config').onclick = async () => {
      const button = $('#test-config');
      button.disabled = true;
      $('#test-result').textContent = '正在调用大模型…';
      try {
        const result = await Api.testLlm();
        $('#test-result').textContent = result.message;
        toast('连接成功', 'success');
      } catch (err) {
        $('#test-result').textContent = err.message;
        toast('连接失败', 'error');
      } finally {
        button.disabled = false;
      }
    };
  };

  global.Pages = Pages;
})(window);
