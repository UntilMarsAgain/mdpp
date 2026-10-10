/* mdpp 交互元素的默认行为：点一下就切 `::tabs`。就这一点。
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