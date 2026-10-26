// Pipeline OS site. Everything here is progressive: the page works without it.
(() => {
  const REPO = "ojaseminem/Pipeline-OS";
  const RELEASES = `https://github.com/${REPO}/releases/latest`;
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.documentElement.classList.add("js");

  /* ── Sticky nav hairline ── */
  const nav = document.querySelector(".nav");
  const onScroll = () => nav.classList.toggle("is-stuck", scrollY > 8);
  addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  /* ── Tabs (tour and code), with arrow-key support ── */
  document.querySelectorAll("[data-tabs]").forEach((root) => {
    const tabs = [...root.querySelectorAll('[role="tab"]')];
    const vertical = root.querySelector('[role="tablist"]').getAttribute("aria-orientation") === "vertical";
    let caption = null;
    if (root.classList.contains("tour")) {
      caption = document.createElement("p");
      caption.className = "tour__mcap small";
      caption.setAttribute("aria-hidden", "true");
      root.querySelector(".tour__stage").append(caption);
    }

    const select = (tab, focus) => {
      tabs.forEach((t) => {
        const on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
      });
      if (focus) tab.focus();
      if (caption) caption.textContent = tab.querySelector(".tour__d").textContent;
      tab.scrollIntoView({ block: "nearest", inline: "nearest", behavior: reduceMotion ? "auto" : "smooth" });
    };

    tabs.forEach((tab, i) => {
      tab.addEventListener("click", () => select(tab, false));
      tab.addEventListener("keydown", (e) => {
        const next = vertical ? ["ArrowDown", "ArrowRight"] : ["ArrowRight"];
        const prev = vertical ? ["ArrowUp", "ArrowLeft"] : ["ArrowLeft"];
        let to = null;
        if (next.includes(e.key)) to = tabs[(i + 1) % tabs.length];
        else if (prev.includes(e.key)) to = tabs[(i - 1 + tabs.length) % tabs.length];
        else if (e.key === "Home") to = tabs[0];
        else if (e.key === "End") to = tabs[tabs.length - 1];
        if (to) { e.preventDefault(); select(to, true); }
      });
    });
    if (caption) caption.textContent = tabs[0].querySelector(".tour__d").textContent;
  });

  /* ── Copy buttons ── */
  document.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const scope = btn.closest("[data-copy-scope]");
      const src = [...scope.querySelectorAll("[data-copy-src]")].find((el) => !el.closest("[hidden]"));
      if (!src) return;
      const label = btn.querySelector("span");
      try {
        await navigator.clipboard.writeText(src.innerText.trim());
        btn.classList.add("is-done");
        label.textContent = "Copied";
      } catch {
        label.textContent = "Press Ctrl+C";
        getSelection().selectAllChildren(src);
      }
      setTimeout(() => { btn.classList.remove("is-done"); label.textContent = "Copy"; }, 1800);
    });
  });

  /* ── Detect the visitor's OS ── */
  const detectOS = () => {
    const p = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent || "").toLowerCase();
    const ua = navigator.userAgent.toLowerCase();
    if (/android|iphone|ipad|ipod/.test(ua)) return null;
    if (p.includes("win")) return "windows";
    if (p.includes("mac")) return "mac";
    if (p.includes("linux") || p.includes("x11")) return "linux";
    return null;
  };
  const os = detectOS();
  const osNames = { windows: "Windows", mac: "macOS", linux: "Linux" };
  // The hero button points at the main installer for this OS.
  const heroAsset = { windows: "x64-setup.exe", mac: "aarch64.dmg", linux: ".AppImage" };

  const heroBtn = document.querySelector("[data-os-download]");
  const heroLabel = document.querySelector("[data-os-label]");
  const heroMeta = document.querySelector("[data-os-meta]");
  if (os) {
    heroLabel.textContent = `Download for ${osNames[os]}`;
    document.querySelector(`.plat[data-plat="${os}"]`)?.classList.add("is-mine");
    if (os === "mac") heroMeta.innerHTML = 'Apple Silicon · <a href="#download">Intel Mac?</a>';
  } else {
    heroBtn.href = "#download";
    heroLabel.textContent = "Download";
  }

  /* ── Fill in real installer links from the latest GitHub release ── */
  const mb = (bytes) => `${Math.round(bytes / 1048576)} MB`;
  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((rel) => {
      const version = rel.tag_name.replace(/^v/, "");
      const assets = rel.assets.filter((a) => !a.name.endsWith(".sig") && !a.name.endsWith(".tar.gz"));
      const find = (suffix) => assets.find((a) => a.name.endsWith(suffix));

      document.querySelectorAll("[data-release-version]").forEach((el) => (el.textContent = `v${version}`));
      const date = new Date(rel.published_at).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
      document.querySelectorAll("[data-release-line]").forEach((el) => {
        el.innerHTML = `Version ${version}, released ${date}. <a href="${rel.html_url}">Release notes</a>.`;
      });

      document.querySelectorAll("[data-asset]").forEach((el) => {
        const a = find(el.dataset.asset);
        if (!a) return;
        el.href = a.browser_download_url;
        el.title = `${a.name} · ${mb(a.size)}`;
      });
      if (os) {
        const a = find(heroAsset[os]);
        if (a) {
          heroBtn.href = a.browser_download_url;
          if (os !== "mac") heroMeta.textContent = `v${version} · ${a.name.split(".").pop().toUpperCase()} · ${mb(a.size)}`;
          else heroMeta.innerHTML = `v${version} · Apple Silicon · <a href="#download">Intel Mac?</a>`;
        }
      }
    })
    .catch(() => {
      // Rate-limited or offline: every link already points at the releases page.
      if (os) heroBtn.href = RELEASES;
    });

  /* ── Reveal on scroll ── */
  const revealTargets = document.querySelectorAll(".section__head, .tour, .duo figure, .split > *, .ai > *, .offline > *, .privacy, .themes > *, .plat, .apps__head");
  if ("IntersectionObserver" in window && !reduceMotion) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => { if (e.isIntersecting) { e.target.classList.add("is-in"); io.unobserve(e.target); } });
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
    revealTargets.forEach((el) => { el.classList.add("reveal"); io.observe(el); });
  }

  /* ── The running build: progress and log lines (sample data, as in the app's demo studio) ── */
  if (reduceMotion) return;
  const LOG = [
    "Compiling shader variants (2,140/3,312)",
    "Compiling shader variants (2,688/3,312)",
    "Compiling shader variants (3,312/3,312)",
    "Building scene 1/4: ReactorRoom.unity",
    "Building scene 2/4: Catwalk.unity",
    "Building scene 3/4: Hangar.unity",
    "Building scene 4/4: Credits.unity",
    "Packing assets into sharedassets0.assets",
    "Copying player to Builds/Windows/Voidline.exe",
    "Build succeeded · 0 errors, 2 warnings",
  ];
  const pctEls = document.querySelectorAll("[data-run-pct]");
  const barEls = document.querySelectorAll("[data-run-bar]");
  const lineEls = document.querySelectorAll("[data-run-line]");
  const logEl = document.querySelector("[data-run-log]");
  let step = 0;

  const tick = () => {
    step = (step + 1) % (LOG.length + 2);
    const i = Math.min(step, LOG.length - 1);
    const pct = step >= LOG.length ? 100 : Math.round(64 + (36 * i) / (LOG.length - 1));
    const done = step >= LOG.length - 1;
    if (step === 0) logEl.replaceChildren();
    pctEls.forEach((el) => (el.textContent = done ? "Done" : `${pct}%`));
    barEls.forEach((el) => (el.style.width = `${step === 0 ? 64 : pct}%`));
    lineEls.forEach((el) => (el.textContent = LOG[i]));
    if (step < LOG.length) {
      const li = document.createElement("li");
      li.textContent = LOG[i];
      logEl.append(li);
      while (logEl.children.length > 3) logEl.firstElementChild.remove();
    }
  };
  let timer = setInterval(tick, 1600);
  document.addEventListener("visibilitychange", () => {
    clearInterval(timer);
    if (!document.hidden) timer = setInterval(tick, 1600);
  });
})();
