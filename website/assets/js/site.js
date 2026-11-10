// CodeScan 官网交互：语言切换、主题切换、移动端导航、代码块复制
(function () {
  "use strict";

  var root = document.documentElement;
  var dict = window.CodeScanI18N || {};

  // ---------- 语言：?lang= → localStorage → 浏览器语言 ----------
  function detectLang() {
    var forced = new URLSearchParams(location.search).get("lang");
    if (forced === "zh" || forced === "en") return forced;
    var saved = localStorage.getItem("codescan-lang");
    if (saved === "zh" || saved === "en") return saved;
    var langs = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language || "en"];
    for (var i = 0; i < langs.length; i++) {
      if (String(langs[i]).toLowerCase().indexOf("zh") === 0) return "zh";
    }
    return "en";
  }

  function applyLang(lang) {
    var table = dict[lang] || dict.en;
    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      var value = table[el.getAttribute("data-i18n")];
      if (value === undefined) return;
      if (el.tagName === "META") {
        el.setAttribute("content", value);
      } else {
        el.innerHTML = value;
      }
    });
    document.querySelectorAll("[data-i18n-alt]").forEach(function (el) {
      var value = table[el.getAttribute("data-i18n-alt")];
      if (value !== undefined) el.setAttribute("alt", value);
    });
    if (table["meta.title"]) document.title = table["meta.title"];
    root.setAttribute("lang", lang === "zh" ? "zh-CN" : "en");
    var btn = document.getElementById("lang-toggle");
    if (btn) btn.textContent = lang === "zh" ? "EN" : "中文";
  }

  var currentLang = detectLang();
  applyLang(currentLang);

  var langBtn = document.getElementById("lang-toggle");
  if (langBtn) {
    langBtn.addEventListener("click", function () {
      currentLang = currentLang === "zh" ? "en" : "zh";
      localStorage.setItem("codescan-lang", currentLang);
      applyLang(currentLang);
    });
  }

  // ---------- 主题：auto → light/dark 切换；?theme= 覆盖；持久化 ----------
  var forcedTheme = new URLSearchParams(location.search).get("theme");
  if (forcedTheme === "light" || forcedTheme === "dark") {
    root.setAttribute("data-theme", forcedTheme);
  } else {
    var savedTheme = localStorage.getItem("codescan-theme");
    if (savedTheme === "light" || savedTheme === "dark") root.setAttribute("data-theme", savedTheme);
  }
  var themeBtn = document.getElementById("theme-toggle");
  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      var current = root.getAttribute("data-theme");
      if (!current) {
        current = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
      }
      var next = current === "dark" ? "light" : "dark";
      root.setAttribute("data-theme", next);
      localStorage.setItem("codescan-theme", next);
    });
  }

  // ---------- 移动端导航 ----------
  var toggle = document.querySelector(".nav-toggle");
  var nav = document.querySelector(".site-nav");
  if (toggle && nav) {
    toggle.addEventListener("click", function () {
      var open = nav.classList.toggle("open");
      toggle.setAttribute("aria-expanded", open ? "true" : "false");
    });
    nav.addEventListener("click", function (e) {
      if (e.target.tagName === "A") {
        nav.classList.remove("open");
        toggle.setAttribute("aria-expanded", "false");
      }
    });
  }

  // ---------- 代码块复制按钮 ----------
  document.querySelectorAll("pre.code").forEach(function (block) {
    var btn = document.createElement("button");
    btn.className = "copy-btn";
    btn.textContent = "Copy";
    btn.setAttribute("aria-label", "Copy to clipboard");
    btn.addEventListener("click", function () {
      var text = block.querySelector("code").textContent;
      navigator.clipboard.writeText(text).then(function () {
        btn.textContent = "Copied";
        setTimeout(function () { btn.textContent = "Copy"; }, 1600);
      }).catch(function () {
        btn.textContent = "Failed";
        setTimeout(function () { btn.textContent = "Copy"; }, 1600);
      });
    });
    block.parentNode.insertBefore(btn, block);
  });
})();
