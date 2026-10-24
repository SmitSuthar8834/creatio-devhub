/* Google Analytics 4 for the public site. Every public page loads this file, so
   the measurement ID lives here and nowhere else. admin/ deliberately does not load it.

   Installer links point at github.com, so GA's built-in file_download tracking
   doesn't recognise them (.msi and .dmg aren't on its extension list). Each
   installer click is sent as a `download_installer` event instead. Mark that
   event as a key event in GA to count downloads as conversions. */
(function () {
  var GA_ID = "G-GMQZYPKYH4";
  if (!/^G-[A-Z0-9]+$/.test(GA_ID) || GA_ID === "G-XXXXXXXXXX") return;
  // Only the two live hosts report, so local previews don't pollute the data.
  var host = location.hostname;
  if (host !== "devhub.tatvikaworks.com" && host !== "smitsuthar8834.github.io") return;

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
})();
