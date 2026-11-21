/* openpost editor: preview, split screen, publish, token notice. */
(function () {
  'use strict';

  var editor = document.getElementById('editor');
  if (!editor) return;

  var contentEl = document.getElementById('content');
  var previewEl = document.getElementById('preview-pane');
  var formatEl = document.getElementById('format');
  var btnPreview = document.getElementById('btn-preview');
  var btnSplit = document.getElementById('btn-split');
  var btnPublish = document.getElementById('btn-publish');

  function api(path, opts) {
    return fetch(path, Object.assign({
      headers: { 'Content-Type': 'application/json' }
    }, opts)).then(function (res) {
      return res.json().catch(function () { return {}; }).then(function (body) {
        if (!res.ok) {
          throw new Error(body.error || ('HTTP ' + res.status));
        }
        return body;
      });
    });
  }

  function renderPreview() {
    return api('/api/preview', {
      method: 'POST',
      body: JSON.stringify({
        content_format: formatEl.value,
        content: contentEl.value
      })
    }).then(function (data) {
      // Safe: the server sanitizes the rendered HTML.
      previewEl.innerHTML = data.html;
    });
  }

  // Debounced live preview while in split-screen mode.
  var debounceTimer = null;
  contentEl.addEventListener('input', function () {
    if (!editor.classList.contains('split')) return;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(renderPreview, 400);
  });

  // Tab inserts two spaces instead of moving focus.
  contentEl.addEventListener('keydown', function (e) {
    if (e.key === 'Tab') {
      e.preventDefault();
      var start = contentEl.selectionStart;
      var end = contentEl.selectionEnd;
      contentEl.value = contentEl.value.slice(0, start) + '  ' + contentEl.value.slice(end);
      contentEl.selectionStart = contentEl.selectionEnd = start + 2;
    }
  });

  // Single-pane preview toggle.
  btnPreview.addEventListener('click', function () {
    var on = editor.classList.toggle('preview-mode');
    previewEl.classList.toggle('hidden', !on);
    if (on) renderPreview().catch(function () {});
  });

  // Split-screen toggle.
  btnSplit.addEventListener('click', function () {
    var on = editor.classList.toggle('split');
    if (on) {
      previewEl.classList.remove('hidden');
      renderPreview().catch(function () {});
    } else if (!editor.classList.contains('preview-mode')) {
      previewEl.classList.add('hidden');
    }
  });

  function clean(v) {
    v = (v || '').trim();
    return v === '' ? null : v;
  }

  function collectPayload() {
    return {
      title: document.getElementById('title').value.trim(),
      content: contentEl.value,
      content_format: formatEl.value,
      author_name: clean(document.getElementById('author-name').value),
      author_url: clean(document.getElementById('author-url').value)
    };
  }

  btnPublish.addEventListener('click', function () {
    var payload = collectPayload();
    if (!payload.title) {
      alert('Title is required.');
      return;
    }
    btnPublish.disabled = true;
    var path = editor.dataset.pagePath;
    var request = path
      ? api('/api/pages/' + encodeURIComponent(path), {
          method: 'PUT',
          body: JSON.stringify(payload)
        })
      : api('/api/pages', {
          method: 'POST',
          body: JSON.stringify(payload)
        });
    request.then(function (data) {
      if (data.token && !localStorage.getItem('openpost_token_notice')) {
        showTokenNotice(data.token, data.path);
      } else {
        window.location.href = '/' + data.path;
      }
    }).catch(function (err) {
      alert(err.message);
      btnPublish.disabled = false;
    });
  });

  function showTokenNotice(token, path) {
    var box = document.getElementById('token-notice');
    document.getElementById('token-value').textContent = token;
    box.classList.remove('hidden');
    // Bring the notice into view: it sits at the top of the editor, but the
    // user may be scrolled deep into a long post when they hit publish.
    box.scrollIntoView({ behavior: 'smooth', block: 'center' });
    // Only show this once per browser.
    localStorage.setItem('openpost_token_notice', '1');

    document.getElementById('btn-copy-token').addEventListener('click', function () {
      navigator.clipboard.writeText(token).then(function () {
        var b = document.getElementById('btn-copy-token');
        b.textContent = 'Copied!';
        setTimeout(function () { b.textContent = 'Copy token'; }, 1500);
      });
    });
    document.getElementById('btn-go').addEventListener('click', function () {
      window.location.href = '/' + path;
    });
    btnPublish.disabled = false;
  }
})();
