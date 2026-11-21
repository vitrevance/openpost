/* openpost — My posts: delete owned posts via the API (cookie auth). */
(function () {
  'use strict';

  function deletePage(path) {
    return fetch('/api/pages/' + encodeURIComponent(path), {
      method: 'DELETE'
    }).then(function (res) {
      if (res.status === 204) return null; // success: empty body
      return res.json().catch(function () { return {}; }).then(function (body) {
        if (!res.ok) {
          throw new Error(body.error || ('HTTP ' + res.status));
        }
        return body;
      });
    });
  }

  document.querySelectorAll('.delete-btn').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var path = btn.dataset.path;
      var row = btn.closest('tr');
      var titleLink = row && row.querySelector('td a');
      var name = titleLink ? titleLink.textContent : path;
      if (!window.confirm('Delete "' + name + '"? This cannot be undone.')) return;
      btn.disabled = true;
      deletePage(path).then(function () {
        var table = row.closest('table');
        row.remove();
        // Last post removed: reload so the "No posts yet" empty state shows.
        if (!table || !table.querySelector('tbody tr')) {
          window.location.reload();
        }
      }).catch(function (err) {
        alert(err.message);
        btn.disabled = false;
      });
    });
  });
})();
