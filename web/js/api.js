/* 接口封装:统一处理 { code, message, data } 响应体 */
(function (global) {
  const BASE = '/api';

  class ApiError extends Error {
    constructor(message, code) {
      super(message);
      this.code = code;
    }
  }

  async function request(method, path, body, options = {}) {
    const init = { method, headers: {} };
    if (body instanceof FormData) {
      init.body = body;
    } else if (body !== undefined) {
      init.headers['Content-Type'] = 'application/json';
      init.body = JSON.stringify(body);
    }
    if (options.signal) init.signal = options.signal;

    let response;
    try {
      response = await fetch(BASE + path, init);
    } catch (err) {
      throw new ApiError('无法连接本地服务,请确认服务仍在运行', -1);
    }

    let payload = null;
    const text = await response.text();
    if (text) {
      try { payload = JSON.parse(text); } catch (err) { payload = null; }
    }
    if (!payload) {
      throw new ApiError(`服务返回异常(HTTP ${response.status})`, response.status);
    }
    if (payload.code !== 0) {
      throw new ApiError(payload.message || '请求失败', payload.code);
    }
    return payload.data;
  }

  /* SSE 流式接口:POST + 逐字读取 */
  async function stream(path, body, handlers = {}) {
    const response = await fetch(BASE + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: handlers.signal,
    });
    if (!response.ok && response.status !== 200) {
      let message = `服务返回异常(HTTP ${response.status})`;
      try {
        const payload = await response.json();
        if (payload && payload.message) message = payload.message;
      } catch (err) { /* ignore */ }
      throw new ApiError(message, response.status);
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder('utf-8');
    let buffer = '';
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let index;
      while ((index = buffer.indexOf('\n\n')) >= 0) {
        const raw = buffer.slice(0, index);
        buffer = buffer.slice(index + 2);
        const event = { name: 'message', data: '' };
        raw.split('\n').forEach((line) => {
          if (line.startsWith('event:')) event.name = line.slice(6).trim();
          else if (line.startsWith('data:')) event.data += line.slice(5).trim();
        });
        if (!event.data) continue;
        let parsed = null;
        try { parsed = JSON.parse(event.data); } catch (err) { parsed = event.data; }
        if (event.name === 'delta' && handlers.onDelta) handlers.onDelta(parsed.text || '');
        else if (event.name === 'done' && handlers.onDone) handlers.onDone(parsed);
        else if (event.name === 'error' && handlers.onError) handlers.onError(parsed);
      }
    }
  }

  global.Api = {
    ApiError,
    health: () => request('GET', '/health'),
    meta: () => request('GET', '/meta'),
    getConfig: () => request('GET', '/config'),
    updateConfig: (data) => request('PUT', '/config', data),
    testLlm: () => request('POST', '/config/test'),

    startInterview: (data) => request('POST', '/interview/start', data),
    answerInterview: (id, answer) => request('POST', `/interview/${id}/answer`, { answer }),
    answerInterviewStream: (id, answer, handlers) => stream(`/interview/${id}/answer/stream`, { answer }, handlers),
    skipQuestion: (id) => request('POST', `/interview/${id}/skip`),
    completeInterview: (id) => request('POST', `/interview/${id}/complete`),
    abandonInterview: (id) => request('POST', `/interview/${id}/abandon`),
    interviewDetail: (id) => request('GET', `/interview/${id}/detail`),
    interviewResult: (id) => request('GET', `/interview/${id}/result`),
    retryReview: (id) => request('POST', `/interview/${id}/review/retry`),
    deleteInterview: (id) => request('DELETE', `/interview/${id}`),
    history: (page = 1, size = 10) => request('GET', `/interview/history?page=${page}&size=${size}`),
    stats: () => request('GET', '/interview/stats'),
    improvementPlan: () => request('GET', '/interview/improvement-plan'),
    regeneratePlan: () => request('POST', '/interview/improvement-plan/regenerate'),

    resumeList: () => request('GET', '/resume/list'),
    resumeDetail: (id) => request('GET', `/resume/${id}`),
    resumeFromText: (data) => request('POST', '/resume/text', data),
    resumeUpload: (file, targetPosition) => {
      const form = new FormData();
      form.append('file', file);
      if (targetPosition) form.append('targetPosition', targetPosition);
      return request('POST', '/resume/upload', form);
    },
    resumeDelete: (id) => request('DELETE', `/resume/${id}`),
    resumeDiagnose: (id, data) => request('POST', `/resume/${id}/diagnosis`, data),
    resumeOptimize: (id, data) => request('POST', `/resume/${id}/optimize`, data),
    resumeStar: (id, data) => request('POST', `/resume/${id}/star`, data),

    salary: (position, city, experience) =>
      request('GET', `/salary/estimate?position=${encodeURIComponent(position)}&city=${encodeURIComponent(city || '')}&experience=${encodeURIComponent(experience || '')}`),
  };
})(window);
