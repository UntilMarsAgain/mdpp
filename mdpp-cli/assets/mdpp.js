/* mdpp 交互元素的**最简**默认行为。
 *
 * 只有一样东西要接线：**`::tabs` 的选项卡**。点一下就切 —— 就这一点。
 *
 * ## 为什么只有这一点
 *
 * 官方二进制只给「最小可用」的那部分：没有它，选项卡点不动，页面是残的；
 * 有它，页面就是好的。往上加的东西（方向键循环、动画、记住上一次选中的那一栏、
 * 深链接到某几栏）都属于**各自的界面**，不该由一个通用工具替所有人定 ——
 * 想加就在自己的页面里加，或者用 `mdpp --js 你的.js` 附带进来。
 *
 * ## 两条不能省的
 *
 * - **能重复跑**：同一段脚本可能被内联不止一次（页面重新渲染？调用方又拼了一遍？）。
 *   所以每个容器先看有没有被打过标记，有就跳过。
 * - **能晚到**：内联的 `<script>` 会在解析到它那一刻就跑，那时后面的选项卡还不存在。
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

  function bindAll(root) {
    root.querySelectorAll(".tabs").forEach(bind);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      bindAll(document);
    });
  } else {
    bindAll(document);
  }
})();