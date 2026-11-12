/* ==========================================================================
   原型画板脚本
   1) 按容器宽度等比缩放页面，支持单页 1:1 放大
   2) 把 [data-qr] 渲染成示意图案（定位角 + 时序线 + 伪数据点）
   ========================================================================== */
(function () {
  var frames = [];

  /* ---------------- 示意图案 ---------------- */
  function qrMarkup() {
    var n = 25, quiet = 2, m = 6;
    var W = (n + quiet * 2) * m;
    var cells = [];
    var seed = 20261110;

    function rnd() {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    }
    function dark(x, y) { cells.push([x, y]); }

    // 三个定位角（7×7：外框 + 中心 3×3）
    [[0, 0], [n - 7, 0], [0, n - 7]].forEach(function (f) {
      for (var y = 0; y < 7; y += 1) {
        for (var x = 0; x < 7; x += 1) {
          var edge = x === 0 || x === 6 || y === 0 || y === 6;
          var core = x >= 2 && x <= 4 && y >= 2 && y <= 4;
          if (edge || core) dark(f[0] + x, f[1] + y);
        }
      }
    });

    function inFinder(x, y) {
      return (x < 8 && y < 8) || (x > n - 9 && y < 8) || (x < 8 && y > n - 9);
    }

    // 对齐图案 5×5
    for (var ay = 16; ay < 21; ay += 1) {
      for (var ax = 16; ax < 21; ax += 1) {
        var e = ax === 16 || ax === 20 || ay === 16 || ay === 20;
        if (e || (ax === 18 && ay === 18)) dark(ax, ay);
      }
    }
    function inAlign(x, y) { return x >= 15 && x <= 21 && y >= 15 && y <= 21; }

    // 时序线
    for (var i = 8; i < n - 8; i += 2) { dark(i, 6); dark(6, i); }

    // 伪数据点
    for (var y2 = 0; y2 < n; y2 += 1) {
      for (var x2 = 0; x2 < n; x2 += 1) {
        if (inFinder(x2, y2) || inAlign(x2, y2)) continue;
        if (y2 === 6 || x2 === 6) continue;
        if (rnd() > 0.53) dark(x2, y2);
      }
    }

    var out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ' + W + ' ' + W + '" width="' + W + '" height="' + W + '">',
      '<rect width="' + W + '" height="' + W + '" fill="#ffffff"/>'];
    cells.forEach(function (c) {
      out.push('<rect x="' + ((c[0] + quiet) * m) + '" y="' + ((c[1] + quiet) * m) + '" width="' + m + '" height="' + m + '" fill="#141619"/>');
    });
    out.push('</svg>');
    return out.join('');
  }

  function paintQr() {
    var nodes = document.querySelectorAll('[data-qr]');
    for (var i = 0; i < nodes.length; i += 1) {
      if (!nodes[i].querySelector('svg')) nodes[i].innerHTML = qrMarkup();
    }
  }

  function fit(frame) {
    var w = Number(frame.dataset.w || 1240);
    var h = Number(frame.dataset.h || 860);
    var inner = frame.querySelector('.frame-inner');
    if (!inner) return;
    var avail = frame.clientWidth;
    var cell = frame.closest('.cell');
    var zoomed = cell && cell.classList.contains('zoom');
    var s = avail / w;
    if (zoomed && s > 1) s = 1;
    inner.style.transform = 'scale(' + s + ')';
    frame.style.height = Math.round(h * s) + 'px';
  }

  function fitAll() {
    for (var i = 0; i < frames.length; i += 1) fit(frames[i]);
  }

  function init() {
    paintQr();

    var list = document.querySelectorAll('.frame');
    for (var i = 0; i < list.length; i += 1) {
      (function (f) {
        frames.push(f);
        var cell = f.closest('.cell');
        var btn = cell && cell.querySelector('.zoombtn');
        if (btn) {
          btn.addEventListener('click', function () {
            var on = cell.classList.toggle('zoom');
            btn.textContent = on ? '缩小' : '1:1';
            requestAnimationFrame(fitAll);
          });
        }
      })(list[i]);
    }

    window.addEventListener('resize', fitAll);
    fitAll();
    setTimeout(fitAll, 100);
    setTimeout(fitAll, 400);

    var themeBtn = document.getElementById('themeBtn');
    if (themeBtn) {
      themeBtn.addEventListener('click', function () {
        var dark = document.documentElement.dataset.theme === 'dark';
        document.documentElement.dataset.theme = dark ? 'light' : 'dark';
        themeBtn.textContent = dark ? '切换深色' : '切换浅色';
      });
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
