/* ==========================================================================
   Auth-screen enhancer — Tira Mail
   --------------------------------------------------------------------------
   Progressive only. The login and signup forms are plain HTML and submit
   without this file. What it adds:

     * a password reveal button (injected, so no-JS users never see a dead
       control; its accessible name comes from `data-reveal-show` /
       `data-reveal-hide`, which the server renders in the page language),
     * a confirm-mismatch hint,
     * a Caps Lock warning on password fields.

   No network, no storage, no third party. The SVG strings below carry no
   xmlns on purpose: they are parsed in an HTML context, and the file is
   checked for external URLs by a test in assets.rs.
   ========================================================================== */

(function () {
  "use strict";

  var EYE =
    '<svg viewBox="0 0 24 24" width="1em" height="1em" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/><circle cx="12" cy="12" r="3"/></svg>';

  var EYE_OFF =
    '<svg viewBox="0 0 24 24" width="1em" height="1em" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143"/><path d="m2 2 20 20"/></svg>';

  /* -------------------------------------------------------------- reveal */
  function addRevealButtons() {
    var inputs = document.querySelectorAll('input[type="password"][data-reveal]');
    Array.prototype.forEach.call(inputs, function (input) {
      var control = input.parentNode;
      if (!control || String(control.className).indexOf("field__control") === -1) {
        return;
      }

      var showLabel = input.getAttribute("data-reveal-show") || "Show password";
      var hideLabel = input.getAttribute("data-reveal-hide") || "Hide password";

      var button = document.createElement("button");
      button.type = "button";
      button.className = "field__toggle";
      button.setAttribute("aria-label", showLabel);
      button.setAttribute("aria-pressed", "false");
      button.innerHTML = EYE;

      button.addEventListener("click", function () {
        var show = input.type === "password";
        input.type = show ? "text" : "password";
        button.setAttribute("aria-pressed", show ? "true" : "false");
        button.setAttribute("aria-label", show ? hideLabel : showLabel);
        button.innerHTML = show ? EYE_OFF : EYE;
        input.focus();
      });

      control.appendChild(button);
    });
  }

  /* ------------------------------------------------------ confirm mismatch */
  function confirmMismatch() {
    var password = document.querySelector("[data-pw]");
    var confirm = document.querySelector("[data-pw-confirm]");
    var hint = document.querySelector("[data-pw-mismatch]");
    if (!password || !confirm) {
      return;
    }

    var sync = function () {
      var bad = confirm.value.length > 0 && confirm.value !== password.value;
      confirm.classList.toggle("is-invalid", bad);
      confirm.setAttribute("aria-invalid", bad ? "true" : "false");
      if (hint) {
        hint.classList.toggle("is-shown", bad);
      }
    };

    confirm.addEventListener("input", sync);
    password.addEventListener("input", sync);
    sync();
  }

  /* ------------------------------------------------------------ caps lock */
  function capsLock() {
    var inputs = document.querySelectorAll('input[type="password"]');
    Array.prototype.forEach.call(inputs, function (input) {
      var field = input.closest ? input.closest(".field") : null;
      var warn = field ? field.querySelector("[data-caps]") : null;
      if (!warn) {
        return;
      }

      var check = function (event) {
        if (typeof event.getModifierState !== "function") {
          return;
        }
        warn.classList.toggle("is-shown", event.getModifierState("CapsLock"));
      };

      input.addEventListener("keydown", check);
      input.addEventListener("keyup", check);
      input.addEventListener("blur", function () {
        warn.classList.remove("is-shown");
      });
    });
  }

  function start() {
    addRevealButtons();
    confirmMismatch();
    capsLock();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
