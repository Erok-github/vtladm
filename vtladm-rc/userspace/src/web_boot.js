(function () {
  "use strict";
  function normPath(u) {
    if (u == null || u === "") return "/";
    var s = String(u).replace(/\/+$/, "");
    return s === "" ? "/" : s;
  }
  function runNav() {
    var p = normPath(location.pathname);
    document.querySelectorAll(".adm-side a[data-nav]").forEach(function (a) {
      if (normPath(a.getAttribute("data-nav")) === p) a.classList.add("on");
    });
  }
  function showToast(msg) {
    var d = document.createElement("div");
    d.className = "vtl-toast";
    d.setAttribute("role", "status");
    d.textContent = msg;
    document.body.appendChild(d);
    requestAnimationFrame(function () {
      d.classList.add("vtl-toast-visible");
    });
    setTimeout(function () {
      d.classList.add("vtl-toast-out");
      setTimeout(function () {
        if (d.parentNode) d.parentNode.removeChild(d);
      }, 380);
    }, 2800);
  }
  window.showToast = showToast;
  runNav();
})();
