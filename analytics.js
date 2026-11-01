/* Google Analytics 4 for the public site. Every public page loads this file, so
   the measurement ID lives here and nowhere else. admin/ deliberately does not load it.

   GA sets cookies, so it only loads after the visitor accepts the banner below.
   The choice is kept in localStorage; privacy.html reopens the banner through
   window.ccAnalyticsChoice(). A decline also clears any _ga cookies already set.

   Installer links point at github.com, so GA's built-in file_download tracking
   doesn't recognise them (.msi and .dmg aren't on its extension list). Each
   installer click is sent as a `download_installer` event instead. Mark that
   event as a key event in GA to count downloads as conversions. */
(function () {
  var GA_ID = "G-GMQZYPKYH4";
  var KEY = "cc-analytics-consent";
  // Only the two live hosts report, so local previews don't pollute the data.
  var host = location.hostname;
  var live = host === "devhub.tatvikaworks.com" || host === "smitsuthar8834.github.io";
  var loaded = false;

  function getChoice() { try { return localStorage.getItem(KEY); } catch (e) { return null; } }
  function setChoice(v) { try { localStorage.setItem(KEY, v); } catch (e) {} }

  function clearGaCookies() {
    document.cookie.split(";").forEach(function (c) {
      var name = c.split("=")[0].trim();
      if (!/^_ga/.test(name)) return;
      var parts = host.split(".");
      for (var i = 0; i < parts.length; i++) {
        var d = parts.slice(i).join(".");
        document.cookie = name + "=; Max-Age=0; path=/; domain=" + d;
      }
      document.cookie = name + "=; Max-Age=0; path=/";
    });
  }

  function loadGa() {
    if (loaded || !live || !/^G-[A-Z0-9]+$/.test(GA_ID)) return;
    loaded = true;
    window.dataLayer = window.dataLayer || [];
    function gtag() { window.dataLayer.push(arguments); }
    window.gtag = gtag;
    gtag("js", new Date());
    gtag("config", GA_ID);

    var s = document.createElement("script");
    s.async = true;
    s.src = "https://www.googletagmanager.com/gtag/js?id=" + encodeURIComponent(GA_ID);
    document.head.appendChild(s);

    document.addEventListener("click", function (e) {
      var a = e.target.closest && e.target.closest("a[href]");
      if (!a || !/github\.com\/SmitSuthar8834\/creatio-devhub\/releases\//.test(a.href)) return;
      var ext = (a.href.match(/\.(exe|msi|dmg)$/i) || [])[1];
      gtag("event", "download_installer", {
        file_extension: ext ? ext.toLowerCase() : "release-page",
        platform: ext && ext.toLowerCase() === "dmg" ? "macos" : ext ? "windows" : "unknown",
        link_id: a.id || undefined,
        link_url: a.href
      });
    });
  }

  function privacyHref() {
    var s = document.querySelector('script[src$="analytics.js"]');
    return s ? s.getAttribute("src").replace(/analytics\.js$/, "privacy") : "privacy";
  }

  function showBanner() {
    if (document.getElementById("cc-consent")) return;
    var b = document.createElement("div");
    b.id = "cc-consent";
    b.setAttribute("role", "dialog");
    b.setAttribute("aria-label", "Analytics cookies");
    b.style.cssText = "position:fixed;left:16px;right:16px;bottom:16px;z-index:50;max-width:560px;margin:0 auto;" +
      "background:var(--surface,#fff);color:var(--text,#12141a);border:1px solid var(--line-2,#d3d7df);border-radius:14px;" +
      "padding:16px 18px;box-shadow:0 12px 32px -12px rgba(0,0,0,.35);font:14.5px/1.55 var(--font,system-ui,sans-serif);" +
      "display:flex;flex-wrap:wrap;gap:12px 16px;align-items:center";
    b.innerHTML =
      '<p style="margin:0;flex:1 1 280px;color:var(--muted,#555b69)">Can we use Google Analytics cookies to count visits and downloads? ' +
      'Nothing is set unless you accept. <a href="' + privacyHref() + '" style="color:var(--brand,#4f46e5)">Privacy policy</a></p>' +
      '<div style="display:flex;gap:8px">' +
      '<button type="button" data-v="denied" style="font:600 14px/1 var(--font,system-ui);padding:10px 14px;border-radius:10px;cursor:pointer;' +
      'background:var(--surface,#fff);color:var(--text,#12141a);border:1px solid var(--line-2,#d3d7df)">Decline</button>' +
      '<button type="button" data-v="granted" style="font:600 14px/1 var(--font,system-ui);padding:10px 14px;border-radius:10px;cursor:pointer;' +
      'background:var(--brand,#4f46e5);color:var(--on-brand,#fff);border:1px solid transparent">Accept</button></div>';
    b.addEventListener("click", function (e) {
      var v = e.target && e.target.getAttribute && e.target.getAttribute("data-v");
      if (!v) return;
      setChoice(v);
      b.remove();
      if (v === "granted") loadGa(); else clearGaCookies();
    });
    document.body.appendChild(b);
  }

  window.ccAnalyticsChoice = showBanner;

  if (getChoice() === "granted") loadGa();
  else if (getChoice() !== "denied") {
    if (document.body) showBanner();
    else document.addEventListener("DOMContentLoaded", showBanner);
  }
})();
