/* mdpp 交互元素的默认行为：选项卡/变体组、导航框折叠、按选项延迟挂载
 * CSS/JS，以及 mermaid / KaTeX / highlight.js 的初始化。
 *
 * 动态资源只做 DOM 挂载，不请求任何接口：单文件模式内容在 <template> 里，
 * 站点模式指向静态 assets/ 文件。
 *
 * 目录：
 *   1. 环境与共享状态
 *   2. 组状态与选项切换
 *   3. 延迟资源挂载
 *   4. 主题跟随
 *   5. navbox
 *   6. mermaid / KaTeX / highlight.js
 *   7. 公共 API
 *   8. 启动
 *
 * 两条不能省：
 * - **能重复跑**：同一段脚本可能被内联不止一次，所以每个容器先看有没有被打过标记；
 * - **能晚到**：内联的 <script> 跑到它那一刻，后面的选项卡还不存在，
 *   所以 document.readyState 是 loading 时挂 DOMContentLoaded，否则立刻执行。
 */
(function () {
  "use strict";

  var PAGE_MARK = "data-mdpp-page-bound";
  var THEME_ATTR = "data-mdpp-theme";
  var MOUNTED = "data-mdpp-mounted";
  var BOUND = "data-mdpp-group-bound";

  var self = document.currentScript;
  var ENV = {
    version: (self && self.getAttribute("data-mdpp-version")) || "",
    mode: (self && self.getAttribute("data-mdpp-mode")) || ""
  };
  if (!window.mdppEnv) {
    window.mdppEnv = ENV;
  }

  // ===== 1. 环境与共享状态 ============================================

  // 组 ID -> 容器；组 ID -> 当前选项；资源 key -> 运行记录。
  // 用无原型对象：ID 来自作者，`__proto__` 之类的键不该污染查找表。
  var groupEls = Object.create(null);
  var active = Object.create(null);
  var runs = Object.create(null);
  var listeners = [];

  function currentTheme() {
    var html = document.documentElement;
    var attr = html.getAttribute(THEME_ATTR);
    if (attr === "light" || attr === "dark") {
      return attr;
    }
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }

  function groupId(group) {
    return group.getAttribute("data-mdpp-group") || "";
  }

  function groupOf(el) {
    return el.closest("[data-mdpp-group]");
  }

  /** 只取属于 group 自己的控件/面板，不碰嵌套组。 */
  function ownElements(group, attribute) {
    var out = [];
    group.querySelectorAll("[" + attribute + "]").forEach(function (el) {
      if (groupOf(el) === group) {
        out.push(el);
      }
    });
    return out;
  }

  function optionValue(el) {
    return el.getAttribute("data-mdpp-option") || "";
  }

  function panelValue(el) {
    return el.getAttribute("data-mdpp-panel") || "";
  }

  function emitChange(group, value, previous) {
    var detail = { group: group, value: value, previous: previous };
    document.dispatchEvent(new CustomEvent("mdpp:variantchange", { detail: detail }));
    listeners.forEach(function (listener) {
      listener(detail);
    });
  }

  function emitAsset(kind, asset, run, mounted) {
    document.dispatchEvent(new CustomEvent(kind, {
      detail: {
        group: asset.getAttribute("data-mdpp-group") || "",
        value: asset.getAttribute("data-mdpp-value") || "",
        key: asset.getAttribute("data-mdpp-key") || "",
        run: run,
        mounted: mounted
      }
    }));
  }

  // ===== 2. 组状态与选项切换 ==========================================

  function setGroupState(group, value, silent) {
    var id = groupId(group);
    var previous = active[id];
    active[id] = value;

    ownElements(group, "data-mdpp-option").forEach(function (el) {
      var on = optionValue(el) === value;
      if (el.getAttribute("role") === "tab") {
        el.setAttribute("aria-selected", on ? "true" : "false");
      }
      if (el.classList.contains("mdpp-tabs__tab")) {
        el.classList.toggle("mdpp-tabs__tab--on", on);
      }
    });

    ownElements(group, "data-mdpp-panel").forEach(function (el) {
      if (panelValue(el) === value) {
        el.removeAttribute("hidden");
      } else {
        el.setAttribute("hidden", "");
      }
    });

    if (!silent && previous !== value) {
      emitChange(id, value, previous);
    }
  }

  function resolveInitial(group) {
    var control = group.getAttribute("data-mdpp-control") || "manual";
    if (control === "theme") {
      return currentTheme();
    }
    var declared = group.getAttribute("data-mdpp-default");
    if (declared !== null) {
      return declared;
    }
    var first = ownElements(group, "data-mdpp-option")[0];
    return first ? optionValue(first) : "";
  }

  function scanGroups(root) {
    root.querySelectorAll("[data-mdpp-group]").forEach(function (group) {
      var id = groupId(group);
      groupEls[id] = group;
      if (!(id in active)) {
        active[id] = resolveInitial(group);
      }
      setGroupState(group, active[id], true);

      if (group.getAttribute("data-mdpp-control") === "theme" || group.hasAttribute(BOUND)) {
        return;
      }
      group.setAttribute(BOUND, "");
      ownElements(group, "data-mdpp-option").forEach(function (el) {
        el.addEventListener("click", function () {
          setGroupState(group, optionValue(el), false);
          refreshAssets();
        });
      });
    });
  }

  // ===== 3. 延迟资源挂载 ==============================================

  function ancestorPanelsActive(asset) {
    var node = asset.parentElement;
    while (node) {
      if (node.hasAttribute && node.hasAttribute("data-mdpp-panel")) {
        var group = groupOf(node);
        if (!group || active[groupId(group)] !== panelValue(node)) {
          return false;
        }
      }
      node = node.parentElement;
    }
    return true;
  }

  function assetActive(asset) {
    var id = asset.getAttribute("data-mdpp-group") || "";
    var value = asset.getAttribute("data-mdpp-value") || "";
    if (id) {
      if (!groupEls[id]) {
        return false;
      }
      if (value !== "*" && active[id] !== value) {
        return false;
      }
    }
    return ancestorPanelsActive(asset);
  }

  function assetContent(asset) {
    if (asset.content && typeof asset.content.textContent === "string") {
      return asset.content.textContent;
    }
    return asset.textContent || "";
  }

  function copyEnv(node, asset) {
    var version = asset.getAttribute("data-mdpp-version") || ENV.version;
    if (version) {
      node.setAttribute("data-mdpp-version", version);
    }
    var group = asset.getAttribute("data-mdpp-group");
    if (group) {
      node.setAttribute("data-mdpp-group", group);
    }
    var value = asset.getAttribute("data-mdpp-value");
    if (value) {
      node.setAttribute("data-mdpp-value", value);
    }
    var key = asset.getAttribute("data-mdpp-key");
    if (key) {
      node.setAttribute("data-mdpp-key", key);
    }
    var runtime = asset.getAttribute("data-mdpp-runtime");
    if (runtime) {
      node.setAttribute("data-mdpp-runtime", runtime);
    }
    node.setAttribute(MOUNTED, "");
  }

  function runRecord(key) {
    if (!runs[key]) {
      runs[key] = { count: 0, done: false };
    }
    return runs[key];
  }

  function hasRun(key) {
    return !!key && !!runs[key] && runs[key].done;
  }

  function runCount(key) {
    return key && runs[key] ? runs[key].count : 0;
  }

  function markRun(key) {
    if (key) {
      runRecord(key).done = true;
    }
  }

  function bumpRun(key) {
    var record = runRecord(key);
    record.count += 1;
    return record.count;
  }

  function mountAsset(asset) {
    var kind = asset.getAttribute("data-mdpp-asset");
    if (kind !== "css" && kind !== "js") {
      return;
    }

    var href = asset.getAttribute("data-mdpp-href");
    var key = asset.getAttribute("data-mdpp-key") || "";
    var once = asset.hasAttribute("data-mdpp-once");

    if (kind === "js" && once && hasRun(key)) {
      return;
    }

    var node;
    if (kind === "css") {
      if (href) {
        node = document.createElement("link");
        node.rel = "stylesheet";
        node.href = href;
        node.setAttribute("data-mdpp-mode", "linked");
      } else {
        node = document.createElement("style");
        node.textContent = assetContent(asset);
        node.setAttribute("data-mdpp-mode", "inline");
      }
    } else {
      node = document.createElement("script");
      node.async = false;
      if (href) {
        node.src = href;
        node.setAttribute("data-mdpp-mode", "linked");
      } else {
        node.textContent = assetContent(asset);
        node.setAttribute("data-mdpp-mode", "inline");
      }
      var run = bumpRun(key);
      node.setAttribute("data-mdpp-run", String(run));
    }

    copyEnv(node, asset);
    // 先记下挂载点再 append：内联脚本执行期间若调用 set/refresh，
    // refreshAssets 不能再把同一份资源挂一遍。
    asset.__mdppMounted = node;

    if (kind === "js" && !href) {
      // 内联脚本 append 后同步执行完，之后才标记；
      // 这样脚本运行期间 hasRun() 仍是第一次的 false。
      document.head.appendChild(node);
      markRun(key);
    } else if (kind === "js") {
      node.addEventListener("load", function () {
        markRun(key);
      });
      document.head.appendChild(node);
    } else {
      document.head.appendChild(node);
    }

    emitAsset("mdpp:variantmount", asset, kind === "js" ? runCount(key) : 0, node);
  }

  function unmountAsset(asset) {
    var node = asset.__mdppMounted;
    if (node && node.parentNode) {
      node.parentNode.removeChild(node);
    }
    asset.__mdppMounted = null;
    emitAsset("mdpp:variantunmount", asset, 0, node || null);
  }

  function refreshAssets() {
    document.querySelectorAll("[data-mdpp-asset]").forEach(function (asset) {
      var shouldMount = assetActive(asset);
      var mounted = asset.__mdppMounted;
      if (shouldMount && !mounted) {
        mountAsset(asset);
      } else if (!shouldMount && mounted) {
        unmountAsset(asset);
      }
    });
  }

  // ===== 4. 主题跟随 ==================================================

  function refreshTheme() {
    var value = currentTheme();
    document.querySelectorAll('[data-mdpp-group][data-mdpp-control="theme"]').forEach(function (group) {
      var id = groupId(group);
      var previous = active[id];
      active[id] = value;
      setGroupState(group, value, true);
      if (previous !== value) {
        emitChange(id, value, previous);
      }
    });
    refreshAssets();
  }

  function bindThemeWatchers() {
    var html = document.documentElement;
    var observer = new MutationObserver(function (mutations) {
      for (var i = 0; i < mutations.length; i++) {
        if (mutations[i].type === "attributes") {
          refreshTheme();
          break;
        }
      }
    });
    observer.observe(html, { attributes: true, attributeFilter: [THEME_ATTR] });

    var media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", function () {
      var pinned = html.getAttribute(THEME_ATTR);
      if (pinned !== "light" && pinned !== "dark") {
        refreshTheme();
      }
    });
  }

  // ===== 5. navbox ====================================================

  function fold(navbox, toggle) {
    var wasCollapsed = toggle.getAttribute("aria-expanded") === "false";
    var body = navbox.querySelector(":scope > .mdpp-navbox__body");
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
    var next = toggle.getAttribute(wasCollapsed ? "data-label-hide" : "data-label-show");
    if (next) {
      toggle.textContent = next;
    }
  }

  function bindNavbox(navbox) {
    var bar = navbox.querySelector(":scope > .mdpp-navbox__bar");
    var toggle = bar ? bar.querySelector(":scope > .mdpp-navbox__toggle") : null;
    if (!toggle || navbox.hasAttribute("data-mdpp-bound")) {
      return;
    }
    navbox.setAttribute("data-mdpp-bound", "");
    toggle.addEventListener("click", function () {
      fold(navbox, toggle);
    });
  }

  // ===== 6. mermaid / KaTeX / highlight.js ==============================

  function bindDiagrams() {
    if (window.mermaid) {
      window.mermaid.initialize({ startOnLoad: false });
      window.mermaid.run({ querySelector: ".mdpp-mermaid" });
    }
    if (window.katex) {
      document.querySelectorAll(".mdpp-math").forEach(function (el) {
        var tex = el.getAttribute("data-tex");
        if (tex) {
          window.katex.render(tex, el, {
            throwOnError: false,
            displayMode: el.classList.contains("mdpp-math--display")
          });
        }
      });
    }
    if (window.hljs) {
      window.hljs.highlightAll();
    }
  }

  // ===== 7. 公共 API ==================================================

  window.mdppVariants = {
    get: function (group) {
      return group in active ? active[group] : null;
    },
    set: function (group, value) {
      var el = groupEls[group];
      if (!el) {
        return false;
      }
      setGroupState(el, value, false);
      refreshAssets();
      return true;
    },
    refresh: function (group) {
      if (group) {
        var el = groupEls[group];
        if (el && el.getAttribute("data-mdpp-control") === "theme") {
          var value = currentTheme();
          var previous = active[group];
          active[group] = value;
          setGroupState(el, value, true);
          if (previous !== value) {
            emitChange(group, value, previous);
          }
        }
      } else {
        refreshTheme();
        return;
      }
      refreshAssets();
    },
    rescan: function () {
      scanGroups(document);
      refreshTheme();
    },
    onChange: function (listener) {
      if (typeof listener !== "function") {
        return function () {};
      }
      listeners.push(listener);
      return function () {
        var at = listeners.indexOf(listener);
        if (at >= 0) {
          listeners.splice(at, 1);
        }
      };
    },
    hasRun: hasRun,
    runCount: runCount,
    markRun: markRun
  };

  // 兼容旧名：下游库重写 data-mdpp-theme 后手动刷新。
  window.mdppThemeInject = function () {
    refreshTheme();
  };

  // ===== 8. 启动 ======================================================

  function bindAll(root) {
    root.querySelectorAll(".mdpp-navbox").forEach(bindNavbox);
    scanGroups(root);
    if (!document.documentElement.hasAttribute(PAGE_MARK)) {
      document.documentElement.setAttribute(PAGE_MARK, "");
      bindThemeWatchers();
      bindDiagrams();
    }
    refreshTheme();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      bindAll(document);
    });
  } else {
    bindAll(document);
  }
})();
