/* mdpp 交互元素的默认行为。
 *
 * 只有一样东西要接线：**`::tabs` 的选项卡**。
 *
 * ## 为什么它在这里而不在渲染器里
 *
 * 渲染器吐的是结构（`role="tablist"` / `aria-selected` / `hidden`），它不该知道
 * 「点一下会发生什么」—— 那是**界面**那一层的事。所以这段脚本住在命令行这边，
 * 由 `--full` 内联进页面；只用片段的人自己决定要不要引它。
 *
 * ## 两条不能省的
 *
 * - **能重复跑**：一段脚本可能被内联不止一次（片段里两个 `::tabs`？脚本是同一个
 *   引用；页面又被重新渲染？），所以每个容器先看有没有被打过标记，有就跳过。
 * - **能晚到**：内联的 `<script>` 会在解析到它那一刻就跑，那时后面的选项卡还不存在。
 *   所以 `document.readyState` 是 `loading` 时挂 `DOMContentLoaded`，否则立刻扫。
 *
 * 键盘照 WAI-ARIA 的 tabs 模式走：左右方向键切换、Tab 键仍能进出这一组。
 */
(function () {
  "use strict";

  var MARK = "data-mdpp-bound";

  function tabsOf(root) {
    return Array.prototype.slice.call(root.querySelectorAll(".tabs"));
  }

  function labelsOf(tabs) {
    return Array.prototype.slice.call(tabs.querySelectorAll(".tabs__tab"));
  }

  function panelsOf(tabs) {
    return Array.prototype.slice.call(tabs.querySelectorAll(".tabs__panel"));
  }

  function select(tabs, index) {
    var labels = labelsOf(tabs);
    var panels = panelsOf(tabs);
    for (var i = 0; i < labels.length; i++) {
      var on = i === index;
      labels[i].classList.toggle("tabs__tab--on", on);
      labels[i].setAttribute("aria-selected", on ? "true" : "false");
      if (panels[i]) {
        // 用 hidden 属性而不是 `style.display` —— 前者能被 CSS 的
        // `@media print` 覆盖掉（见 mdpp.css 里那段），后者不能。
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

    var labels = labelsOf(tabs);
    labels.forEach(function (label, index) {
      label.addEventListener("click", function () {
        select(tabs, index);
      });
    });

    // 方向键在**标签条**那一组里循环；不在这一组里按就别管（那是页面自己的快捷键）
    tabs.addEventListener("keydown", function (event) {
      var delta =
        event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
      if (!delta || labels.length === 0) {
        return;
      }
      var at = labels.indexOf(document.activeElement);
      if (at < 0) {
        return;
      }
      event.preventDefault();
      var next = (at + delta + labels.length) % labels.length;
      labels[next].focus();
      select(tabs, next);
    });
  }

  function bindAll(root) {
    tabsOf(root).forEach(bind);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      bindAll(document);
    });
  } else {
    bindAll(document);
  }
})();