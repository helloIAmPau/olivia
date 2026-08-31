/* OlivIA — site interactions (vanilla, no dependencies) */
(function () {
  "use strict";

  /* ---- Mobile menu ----------------------------------------------------- */
  var navToggle = document.getElementById("nav-toggle");
  var mobileNav = document.getElementById("site-nav-mobile");

  function closeMenu() {
    if (!navToggle || !mobileNav) { return; }
    navToggle.setAttribute("aria-expanded", "false");
    navToggle.setAttribute("aria-label", "Open menu");
    mobileNav.hidden = true;
  }

  if (navToggle && mobileNav) {
    navToggle.addEventListener("click", function () {
      var open = navToggle.getAttribute("aria-expanded") === "true";
      navToggle.setAttribute("aria-expanded", String(!open));
      navToggle.setAttribute("aria-label", open ? "Open menu" : "Close menu");
      mobileNav.hidden = open;
    });
    mobileNav.addEventListener("click", function (e) {
      if (e.target.tagName === "A") { closeMenu(); }
    });
    window.addEventListener("keydown", function (e) {
      if (e.key === "Escape") { closeMenu(); }
    });
    // Auto-close when growing past the mobile breakpoint.
    window.matchMedia("(min-width: 781px)").addEventListener("change", function (e) {
      if (e.matches) { closeMenu(); }
    });
  }

  /* ---- Active section in the nav -------------------------------------- */
  var links = Array.prototype.slice.call(document.querySelectorAll('.site-nav a[href^="#"]'));
  var byId = {};
  links.forEach(function (a) { byId[a.getAttribute("href").slice(1)] = a; });

  var sections = Object.keys(byId)
    .map(function (id) { return document.getElementById(id); })
    .filter(Boolean);

  if ("IntersectionObserver" in window && sections.length) {
    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        var link = byId[entry.target.id];
        if (!link) { return; }
        if (entry.isIntersecting) {
          links.forEach(function (l) { l.classList.remove("active"); l.removeAttribute("aria-current"); });
          link.classList.add("active");
          link.setAttribute("aria-current", "true");
        }
      });
    }, { rootMargin: "-45% 0px -50% 0px", threshold: 0 });

    sections.forEach(function (s) { observer.observe(s); });
  }

  /* ---- Current year (defensive; footer has a static fallback) --------- */
  var y = document.getElementById("year");
  if (y) { y.textContent = String(new Date().getFullYear()); }
})();
