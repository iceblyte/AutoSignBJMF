/* 明暗主题切换：跟随系统，手动选择后记住 */
(function () {
  var KEY = "bjmf-theme";
  var root = document.documentElement;

  try {
    var saved = localStorage.getItem(KEY);
    if (saved === "dark" || saved === "light") root.dataset.theme = saved;
  } catch (e) {
    /* 隐私模式下 localStorage 不可用，忽略 */
  }

  function isDark() {
    if (root.dataset.theme) return root.dataset.theme === "dark";
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  }

  function sync(button) {
    button.setAttribute("aria-label", isDark() ? "切换到浅色" : "切换到深色");
    button.setAttribute("title", isDark() ? "切换到浅色" : "切换到深色");
  }

  document.addEventListener("DOMContentLoaded", function () {
    var button = document.querySelector("[data-theme-toggle]");
    if (!button) return;
    sync(button);
    button.addEventListener("click", function () {
      var next = isDark() ? "light" : "dark";
      root.dataset.theme = next;
      try {
        localStorage.setItem(KEY, next);
      } catch (e) {
        /* 忽略写入失败 */
      }
      sync(button);
    });
  });
})();
