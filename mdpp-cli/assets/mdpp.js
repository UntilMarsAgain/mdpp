/* mdpp 交互元素的默认行为：点一下就切 `::tabs`、折起或展开 `::navbox`。就这两点。
 *
 * 往上加的东西（方向键、动画、记住上次选中、深链接到某一栏）属于各自的界面 ——
 * 在自己的页面里加，或用 `mdpp --js 你的.js` 附带进来。
 *
 * 两条不能省：
 * - **能重复跑**：同一段脚本可能被内联不止一次，所以每个容器先看有没有被打过标记；
 * - **能晚到**：内联的 `<script>` 跑到它那一刻，后面的选项卡还不存在，
 *   所以 `document.readyState` 是 `loading` 时挂 `DOMContentLoaded`，否则立刻扫。
 */
(function () {
  "use strict";

  var MARK = "data-mdpp-bound";

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

  function bindAll(root) {
    root.querySelectorAll(".tabs").forEach(bind);
    root.querySelectorAll(".navbox").forEach(bindNavbox);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      bindAll(document);
    });
  } else {
    bindAll(document);
  }
})();