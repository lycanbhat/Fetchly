// Fetchly website: OS-aware download buttons backed by the latest GitHub release.
(function () {
  const REPO = "lycanbhat/Fetchly";
  const RELEASES = `https://github.com/${REPO}/releases/latest`;

  // ---------------------------------------------------------------- OS detection
  function detectOS() {
    const uaData = navigator.userAgentData;
    const platform = ((uaData && uaData.platform) || navigator.platform || "").toLowerCase();
    const ua = navigator.userAgent.toLowerCase();
    if (/iphone|ipad|ipod/.test(ua) || (platform === "macintel" && navigator.maxTouchPoints > 1)) return "mobile";
    if (/android/.test(ua)) return "mobile";
    // The user-agent string is checked first; `platform` is the fallback.
    if (ua.includes("windows")) return "windows";
    if (ua.includes("mac os")) return "mac";
    if (ua.includes("linux") || ua.includes("x11")) return "linux";
    if (platform.includes("win")) return "windows";
    if (platform.includes("mac")) return "mac";
    if (platform.includes("linux")) return "linux";
    return "unknown";
  }

  const OS = {
    mac: { label: "Download for macOS", icon: "apple", asset: "macos-universal.dmg", meta: "Universal · Apple Silicon & Intel · macOS 11+" },
    windows: { label: "Download for Windows", icon: "monitor", asset: "windows-x64-setup.exe", meta: "Windows 10 & 11 · 64-bit installer" },
    linux: { label: "Download for Linux", icon: "terminal", asset: "_amd64.deb", meta: "Debian / Ubuntu .deb · other distros below" },
  };

  const os = detectOS();
  const info = OS[os];

  const heroBtn = document.getElementById("hero-download");
  const heroLabel = document.getElementById("hero-download-label");
  const heroIcon = document.querySelector("#hero-os-icon use");
  const heroMeta = document.getElementById("hero-meta");
  const ctaBtn = document.getElementById("cta-download");

  if (info) {
    heroLabel.textContent = info.label;
    heroIcon.setAttribute("href", `#i-${info.icon}`);
    heroMeta.textContent = `${info.meta} · Free & open source`;
    ctaBtn.querySelector("span").textContent = info.label;
    const card = document.querySelector(`.dl[data-os="${os}"]`);
    if (card) card.classList.add("current");
  } else {
    // Phones/tablets and unknown systems: point at the download section instead.
    heroLabel.textContent = os === "mobile" ? "Get Fetchly for your computer" : "Download Fetchly";
    heroBtn.setAttribute("href", "#download");
    ctaBtn.setAttribute("href", "#download");
    if (os === "mobile") heroMeta.textContent = "Available for macOS, Windows, Linux and the web";
  }
  // Until release data arrives, OS buttons open the latest release page.
  if (info) { heroBtn.href = RELEASES; ctaBtn.href = RELEASES; }

  // ---------------------------------------------------------------- latest release
  const fmtSize = (bytes) => (bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.round(bytes / 1024)} KB`);

  function applyRelease(rel) {
    const assets = rel.assets || [];
    const find = (suffix) => assets.find((a) => a.name.endsWith(suffix));
    const version = (rel.tag_name || "").replace(/^v/, "");

    document.querySelectorAll("[data-version]").forEach((el) => {
      el.textContent = `Fetchly ${version}`;
    });

    document.querySelectorAll("[data-asset]").forEach((a) => {
      const asset = find(a.getAttribute("data-asset"));
      if (!asset) return;
      a.href = asset.browser_download_url;
      a.setAttribute("download", "");
      const size = a.querySelector(".size");
      if (size) size.textContent = fmtSize(asset.size);
    });

    const sums = find("SHA256SUMS.txt");
    const sumsLink = document.querySelector("[data-sums]");
    if (sumsLink) sumsLink.href = sums ? sums.browser_download_url : RELEASES;

    if (info) {
      const asset = find(info.asset);
      if (asset) {
        heroBtn.href = asset.browser_download_url;
        ctaBtn.href = asset.browser_download_url;
        heroMeta.textContent = `Fetchly ${version} · ${info.meta} · ${fmtSize(asset.size)}`;
      }
    }
  }

  const CACHE_KEY = "fetchly-latest-release";
  try {
    const cached = JSON.parse(sessionStorage.getItem(CACHE_KEY) || "null");
    if (cached) applyRelease(cached);
  } catch (_) { /* storage unavailable */ }

  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((rel) => {
      const slim = { tag_name: rel.tag_name, assets: rel.assets.map(({ name, size, browser_download_url }) => ({ name, size, browser_download_url })) };
      try { sessionStorage.setItem(CACHE_KEY, JSON.stringify(slim)); } catch (_) {}
      applyRelease(slim);
    })
    .catch(() => {
      document.querySelector("[data-sums]")?.setAttribute("href", RELEASES);
      // Links already point at the latest release page, which always works.
    });

  // ---------------------------------------------------------------- reveal on scroll
  const items = document.querySelectorAll(".showcase, .card, .dl, .prop, .table-wrap, .split > *, details");
  if ("IntersectionObserver" in window && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
    items.forEach((el) => el.classList.add("reveal"));
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => {
        if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
      });
    }, { rootMargin: "0px 0px -60px 0px" });
    items.forEach((el) => io.observe(el));
  }

  // Pause the demo video when it's off screen.
  const video = document.getElementById("demo");
  if (video && "IntersectionObserver" in window) {
    new IntersectionObserver(([e]) => (e.isIntersecting ? video.play().catch(() => {}) : video.pause()), { threshold: 0.2 }).observe(video);
  }
})();
