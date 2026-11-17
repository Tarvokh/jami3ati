/* Tira Mail — navigation accessibility enhancer.
 *
 * Self-hosted and progressive. Served from this process under a strict
 * `script-src 'self'` CSP; it is never inline and never calls a network
 * resource. It does one job HTMX cannot do alone: give screen-reader users
 * the same feedback a full page navigation gives them.
 *
 * A boosted link swaps the body over XHR, so the browser never announces a
 * new page. This listens to HTMX's own events and (1) marks #main aria-busy
 * while the swap is in flight and (2) writes a polite status into the
 * persistent sr-only #nav-status live region — "loading", then the arrived
 * page title, or an honest error. The visual pill (#nav-indicator) is driven
 * purely by CSS from the .htmx-request class, so sighted feedback still works
 * if this script never runs. With JavaScript disabled there are no boosted
 * swaps at all: links and forms are ordinary navigations the browser already
 * announces. Nothing here is required for the page to function.
 *
 * Elements are re-queried inside every handler because a boosted swap
 * replaces the body's children (including #main and #nav-status); the
 * listeners live on `document`, which survives the swap.
 */
(function () {
  "use strict";

  function status(text) {
    var region = document.getElementById("nav-status");
    if (region) {
      region.textContent = text;
    }
  }

  function busy(on) {
    var main = document.getElementById("main");
    if (main) {
      main.setAttribute("aria-busy", on ? "true" : "false");
    }
  }

  document.addEventListener("htmx:beforeRequest", function () {
    busy(true);
    status("جارٍ التحميل…");
  });

  document.addEventListener("htmx:afterSettle", function () {
    busy(false);
    // HTMX updates document.title from the response on boost; announcing it
    // mirrors what a full navigation would tell a screen reader.
    status(document.title);
  });

  document.addEventListener("htmx:responseError", function () {
    busy(false);
    status("تعذّر تحميل الصفحة. أعد المحاولة.");
  });

  document.addEventListener("htmx:sendError", function () {
    busy(false);
    status("تعذّر الاتصال بالخادم. أعد المحاولة.");
  });
})();