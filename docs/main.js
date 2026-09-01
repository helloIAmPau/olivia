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

  /* ---- Use-case tabs --------------------------------------------------- */
  var tablists = document.querySelectorAll('[role="tablist"]');
  Array.prototype.forEach.call(tablists, function (tablist) {
    var tabs = Array.prototype.slice.call(tablist.querySelectorAll('[role="tab"]'));

    function select(tab) {
      tabs.forEach(function (t) {
        var selected = t === tab;
        t.setAttribute("aria-selected", String(selected));
        t.classList.toggle("is-active", selected);
        t.tabIndex = selected ? 0 : -1;

        var panel = document.getElementById(t.getAttribute("aria-controls"));
        if (!panel) { return; }
        panel.classList.toggle("is-active", selected);
        panel.hidden = !selected;
      });
    }

    tabs.forEach(function (tab, index) {
      tab.addEventListener("click", function () { select(tab); });
      tab.addEventListener("keydown", function (e) {
        var next = null;
        if (e.key === "ArrowRight" || e.key === "ArrowDown") { next = tabs[(index + 1) % tabs.length]; }
        else if (e.key === "ArrowLeft" || e.key === "ArrowUp") { next = tabs[(index - 1 + tabs.length) % tabs.length]; }
        if (next) { e.preventDefault(); select(next); next.focus(); }
      });
    });
  });

  /* ---- Cat scene (architecture animation) ----------------------------- */
  var cat = document.getElementById("cat");
  var scene = document.querySelector(".cat-scene");

  if (cat && scene) {
    var model = document.getElementById("platform-model");
    var request = scene.querySelector(".request");
    var bubble = request.querySelector(".bubble");
    var BUBBLE_Q = "assets/bubble-question.png";   // ??? — while the request is in flight
    var BUBBLE_X = "assets/bubble-exclaim.png";     // !!! — once the cat is back home
    var img = new Image(); img.src = BUBBLE_X;       // preload the swap frame
    var say = document.getElementById("model-say");
    var tool = document.getElementById("model-tool");
    var TOOLS = ["assets/tool1.png", "assets/tool2.png", "assets/tool3.png"];
    TOOLS.forEach(function (s) { var i = new Image(); i.src = s; });   // preload

    var PX = 80;            // one 16px sprite cell scaled x5
    var FEET = 20;          // transparent px at the bottom of a scaled cell
    var GROUND = 40;        // feet rest on the soil surface (soil strip is 40px)

    var ACTIONS = {
      sleepidle: { row: 12, frames: 1, fps: 1,  loop: false },
      sleep:     { row: 11, frames: 3, fps: 5,  loop: false },
      stretch:   { row: 6,  frames: 6, fps: 9,  loop: false },
      idle:      { row: 1,  frames: 3, fps: 4,  loop: true  },
      sit:       { row: 7,  frames: 2, fps: 4,  loop: false },
      run:       { row: 0,  frames: 4, fps: 12, loop: true  },
      jumpup:    { row: 3,  frames: 2, fps: 9,  loop: false },
      jumpdown:  { row: 4,  frames: 2, fps: 9,  loop: false }
    };

    var frameTimer = null;
    function play(name) {
      var a = ACTIONS[name];
      if (!a) { return; }
      clearInterval(frameTimer);
      cat.style.backgroundPositionY = -(a.row * PX) + "px";
      var f = 0;
      function draw() { cat.style.backgroundPositionX = -(f * PX) + "px"; }
      draw();
      if (a.frames > 1) {
        frameTimer = setInterval(function () {
          f++;
          if (f >= a.frames) {
            if (a.loop) { f = 0; } else { clearInterval(frameTimer); return; }
          }
          draw();
        }, 1000 / a.fps);
      }
    }

    function face(d) { cat.style.transform = "scaleX(" + d + ")"; }
    function wait(ms) { return new Promise(function (r) { setTimeout(r, ms); }); }
    function setPos(x, y) { cat.style.transitionDuration = "0ms"; cat.style.left = x + "px"; cat.style.bottom = y + "px"; }
    function moveTo(x, y, ms, ease) {
      cat.style.transitionTimingFunction = ease || "linear";
      cat.style.transitionDuration = ms + "ms";
      cat.style.left = x + "px";
      cat.style.bottom = y + "px";
      return wait(ms);
    }

    function geom() {
      var boxH = model.offsetHeight || 80;   // hill height (2 tiers)
      var boxW = model.offsetWidth || 160;
      var layerH = boxH / 2;                  // one tier
      var onLeft = model.offsetLeft + boxW / 2 - PX / 2;
      return {
        home: 70,
        ground: GROUND - FEET,               // feet on the soil
        approach: onLeft - 62,               // just short of the hill
        t1Left: onLeft - boxW / 4,           // lower tier, on the left step
        t1Bottom: GROUND + layerH - FEET,    // feet on the lower tier
        onLeft: onLeft,                      // centred on the top tier
        onBox: GROUND + boxH - FEET          // feet on the top of the hill
      };
    }

    // Position a pop element just to the right of the cat on the hilltop, kept inside the scene.
    function place(el) {
      var g = geom();
      el.style.bottom = (g.onBox + FEET + 22) + "px";
      var w = el.offsetWidth || 44;
      var left = g.onLeft + PX + 12;
      var maxLeft = scene.clientWidth - w - 6;
      if (left > maxLeft) { left = maxLeft; }
      el.style.left = left + "px";
    }
    function popHideAll() { say.classList.remove("show"); tool.classList.remove("show"); }
    async function textBeat() {
      place(say); say.classList.add("show"); await wait(820);
      say.classList.remove("show"); await wait(280);
    }
    async function toolBeat(src) {
      tool.src = src; place(tool); tool.classList.add("show"); await wait(820);
      tool.classList.remove("show"); await wait(280);
    }

    function reset() {
      request.classList.remove("in", "leaving");
      bubble.src = BUBBLE_Q;
      popHideAll();
      var g = geom();
      face(1); setPos(g.home, g.ground); play("sleepidle");
    }

    var active = false, running = false;
    var UP = "cubic-bezier(.2,.8,.3,1)", DOWN = "cubic-bezier(.6,0,.8,.4)";

    async function runScene() {
      if (running) { return; }
      running = true;
      while (active) {
        reset(); await wait(600);
        var g = geom();

        request.classList.add("in"); await wait(1200);        // the ??? bubble fades in and bounces
        play("stretch"); await wait(760);                      // olivia wakes

        // climb the hill, one hop per tier
        face(1); play("run"); await moveTo(g.approach, g.ground, 1200);
        play("jumpup"); await moveTo(g.t1Left, g.t1Bottom, 380, UP);  // hop onto the lower tier
        play("jumpup"); await moveTo(g.onLeft, g.onBox, 380, UP);     // hop onto the top tier
        play("idle"); await wait(350);

        // on top: alternate /completions with the three tools — text, image, ... , text
        await textBeat();
        await toolBeat(TOOLS[0]);
        await textBeat();
        await toolBeat(TOOLS[1]);
        await textBeat();
        await toolBeat(TOOLS[2]);
        await textBeat();

        // turn home, climb down one hop per tier, and return to the bubble
        face(-1);
        play("jumpdown"); await moveTo(g.t1Left, g.t1Bottom, 340, DOWN); // drop to the lower tier
        play("jumpdown"); await moveTo(g.approach, g.ground, 340, DOWN); // drop to the ground
        play("run"); await moveTo(g.home, g.ground, 1200);              // run home
        play("sit"); await wait(500);                                  // pause at home
        bubble.src = BUBBLE_X; await wait(300);                        // swap ??? → !!!
        request.classList.remove("in");
        request.classList.add("leaving"); await wait(900);            // slide the bubble off to the left
        face(1); play("sleep"); await wait(700); play("sleepidle");    // done — back to sleep
        await wait(1500);
      }
      running = false;
    }

    var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    reset();
    if (reduce) {
      request.classList.add("in"); place(say); say.classList.add("show");
    } else if ("IntersectionObserver" in window) {
      var io = new IntersectionObserver(function (entries) {
        entries.forEach(function (e) {
          if (e.isIntersecting) { active = true; runScene(); }
          else { active = false; }
        });
      }, { threshold: 0.25 });
      io.observe(scene);
    } else {
      active = true; runScene();
    }
  }


  /* ---- Current year (defensive; footer has a static fallback) --------- */
  var y = document.getElementById("year");
  if (y) { y.textContent = String(new Date().getFullYear()); }
})();
