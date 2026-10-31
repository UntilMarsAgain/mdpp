/* mdpp 交互元素的默认行为：点一下就切 `::tabs`、折起或展开 `::navbox`、按主题
 * 选择性注入 `::theme` 里的 ::css / ::js / ::html js。就这几点。
 *
 * 往上加的东西（方向键、动画、记住上次选中、深链接到某一栏）属于各自的界面 ——
 * 在自己的页面里加，或用 `mdpp --js 你的.js` 附带进来。
 *
 * 两条不能省：
 * - **能重复跑**：同一段脚本可能被内联不止一次，所以每个容器先看有没有被打过标记；
 * - **能晚到**：内联的 `<script>` 跑到它那一刻，后面的选项卡还不存在，
 *   所以 `document.readyState` 是 `loading` 时挂 `DOMContentLoaded`，否则立刻执。
 */
(function () {
  "use strict";

  var MARK = "data-mdpp-bound";
  var THEME_ATTR = "data-mdpp-theme";
  var INJECTED_MARK = "data-mdpp-theme-injected";

  function select(tabs, index) {
    var labels = tabs.querySelectorAll(".tabs__tab");
    var panels = tabs.querySelectorAll(".tabs__panel");
    for (var i = 0; i < labels.length; i++) {
      var on = i === index;
      labels[i].classList.toggle("tabs__tab--on", on);
      labels[i].setAttribute("aria-selected", on ? "true" : "false");
      if (panels[i]) {
        // 用 hidden 属性而不是 style.display —— 前者能被 `@media print` 覆盖掉
        //（见 mdpp.css 里那段），后者不能。
        if (on) {
          panels[i].removeAttribute("hidden");
        } else {
          panels[i].setAttribute("hidden", "");
        }
      }
    }
  }

  function bind(tabs) {
    if (tabs.getAttribute(MARK)) {
      return;
    }
    tabs.setAttribute(MARK, "");
    tabs.querySelectorAll(".tabs__tab").forEach(function (label, index) {
      label.addEventListener("click", function () {
        select(tabs, index);
      });
    });
  }

  function fold(navbox, toggle) {
    // `aria-expanded` 说的是**现在**的样子，所以它就是「刚才是不是折着」
    var wasCollapsed = toggle.getAttribute("aria-expanded") === "false";
    var body = navbox.querySelector(".navbox__body");
    if (!body) {
      return;
    }
    if (wasCollapsed) {
      body.removeAttribute("hidden");
    } else {
      body.setAttribute("hidden", "");
    }
    toggle.setAttribute("aria-expanded", wasCollapsed ? "true" : "false");
    navbox.setAttribute("data-state", wasCollapsed ? "expanded" : "collapsed");
    // 按钮上要写**接下来会发生什么**，所以折着时写「显示」，展开时写「隐藏」。
    // 那两个字由渲染器落在 data-label-* 上（见 stdlib/navbox.rs）—— 中文写在
    // 渲染器那一侧，脚本里就不必硬编码。
    var next = toggle.getAttribute(wasCollapsed ? "data-label-hide" : "data-label-show");
    if (next) {
      toggle.textContent = next;
    }
  }

  function bindNavbox(navbox) {
    var toggle = navbox.querySelector(".navbox__toggle");
    // 嵌进去的（没有 title=）与 plain 的都没有开关，没什么可接的
    if (!toggle || navbox.getAttribute(MARK)) {
      return;
    }
    navbox.setAttribute(MARK, "");
    toggle.addEventListener("click", function () {
      fold(navbox, toggle);
    });
  }

  // ---- ::theme 选择性注入 -------------------------------------------
  // ::theme 节里的 ::css / ::js / ::html js 被渲染为带 data-theme 的
  // .theme-asset 占位容器，这里根据当前主题选择性地把它们真正
  // 注入到文档里（创建 <style> / <script> 标签）。
  //
  // 下游库若重写了 data-mdpp-theme，可调用 window.mdppThemeInject()
  // 手动触发重新注入；系统颜色方案切换时也会自动重新注入。

  function currentTheme() {
    var html = document.documentElement;
    var attr = html.getAttribute(THEME_ATTR);
    if (attr === "light" || attr === "dark") {
      return attr;
    }
    // system 或未写：跟随系统
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }

  function removeInjected() {
    document.querySelectorAll("[" + INJECTED_MARK + "]").forEach(function (el) {
      el.remove();
    });
  }

  function injectThemeAssets(theme) {
    removeInjected();
    document.querySelectorAll(".theme-asset").forEach(function (container) {
      var containerTheme = container.getAttribute("data-theme");
      // "" 表示 both，两边都注入
      if (containerTheme !== theme && containerTheme !== "") {
        return;
      }
      if (container.classList.contains("theme-css")) {
        var style = document.createElement("style");
        style.textContent = container.textContent;
        style.setAttribute(INJECTED_MARK, "");
        document.head.appendChild(style);
      } else if (container.classList.contains("theme-js")) {
        var script = document.createElement("script");
        script.textContent = container.textContent;
        script.setAttribute(INJECTED_MARK, "");
        document.head.appendChild(script);
      }
    });
  }

  function bindTheme() {
    var html = document.documentElement;

    // 监听 data-mdpp-theme 属性变化（下游库重写主题时触发）
    var observer = new MutationObserver(function (mutations) {
      for (var i = 0; i < mutations.length; i++) {
        if (mutations[i].type === "attributes" && mutations[i].attributeName === THEME_ATTR) {
          injectThemeAssets(currentTheme());
          break;
        }
      }
    });
    observer.observe(html, { attributes: true });

    // 监听系统颜色方案变化（只有在未钉死主题时才跟随系统）
    var media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", function () {
      if (!html.hasAttribute(THEME_ATTR)) {
        injectThemeAssets(currentTheme());
      }
    });

    // 初始注入
    injectThemeAssets(currentTheme());
  }

  // 暴露给下游库：手动触发重新注入
  window.mdppThemeInject = function () {
    injectThemeAssets(currentTheme());
  };

  // ---- mermaid / KaTeX -----------------------------------------------
  // 页面里若用了 ::mermaid（pre.mermaid）或 ::math
  //（div.math，带 data-tex 属性），需要把对应的前端脚本
  // 跑起来才能在浏览器里画出来。脚本是由 page.rs 按 external-script
  // 参数从 CDN 写进页面的；这里只负责调用它们的渲染 API。
  function bindDiagrams() {
    if (window.mermaid) {
      window.mermaid.initialize({ startOnLoad: true });
    }
    if (window.katex) {
      document.querySelectorAll(".math").forEach(function (el) {
        var tex = el.getAttribute("data-tex");
        if (tex) {
          window.katex.render(tex, el, { throwOnError: false });
        }
      });
    }
  }

  function bindAll(root) {
    root.querySelectorAll(".tabs").forEach(bind);
    root.querySelectorAll(".navbox").forEach(bindNavbox);
    bindTheme();
    bindDiagrams();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      bindAll(document);
    });
  } else {
    bindAll(document);
  }
})();