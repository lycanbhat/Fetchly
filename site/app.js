// Fetchly website
(function () {
  const REPO = "lycanbhat/Fetchly";
  const RELEASES = `https://github.com/${REPO}/releases/latest`;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ================================================================ OS detection
  function detectOS() {
    const ua = navigator.userAgent.toLowerCase();
    const platform = ((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "").toLowerCase();
    if (/iphone|ipad|ipod|android/.test(ua) || (platform === "macintel" && navigator.maxTouchPoints > 1)) return "mobile";
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
    mac: { name: "macOS", icon: "apple", asset: "macos-universal.dmg", sub: "Universal · Apple Silicon & Intel · macOS 11+", hint: "First launch: right-click Fetchly → Open (builds aren't notarized yet)." },
    windows: { name: "Windows", icon: "windows", asset: "windows-x64-setup.exe", sub: "Installer · Windows 10 & 11 · x64", hint: "If SmartScreen appears: More info → Run anyway." },
    linux: { name: "Linux", icon: "linux", asset: "_amd64.deb", sub: "Debian / Ubuntu .deb · x86_64", hint: "Other distros: grab the .tar.gz and run ./install.sh." },
  };
  const os = detectOS();
  const info = OS[os];

  const heroBtn = $("#hero-download"), heroLabel = $("#hero-download-label"), heroIcon = $("#hero-os-icon use"), heroMeta = $("#hero-meta");
  const ctaBtn = $("#cta-download");
  const dlBtn = $("#dl-hero-btn");

  if (info) {
    heroLabel.textContent = `Download for ${info.name}`;
    heroIcon.setAttribute("href", `#i-${info.icon}`);
    ctaBtn.querySelector("span").textContent = `Download for ${info.name}`;
    $("#dl-hero-icon use").setAttribute("href", `#i-${info.icon}`);
    $("#dl-hero-title").textContent = `Fetchly for ${info.name}`;
    $("#dl-hero-sub").textContent = info.sub;
    $("#dl-hero-hint").textContent = info.hint;
    dlBtn.querySelector("span").textContent = `Download for ${info.name}`;
    $$(`.dl-row[data-os="${os}"]`).forEach((r) => r.classList.add("current"));
  } else {
    const label = os === "mobile" ? "Get it for your computer" : "Download Fetchly";
    heroLabel.textContent = label;
    [heroBtn, ctaBtn, dlBtn].forEach((b) => b.setAttribute("href", "#download"));
    ctaBtn.querySelector("span").textContent = label;
    dlBtn.setAttribute("href", RELEASES);
    dlBtn.querySelector("span").textContent = "See all downloads";
    $("#dl-hero-title").textContent = "Fetchly for every desktop";
    $("#dl-hero-sub").textContent = "macOS · Windows · Linux · Web";
    if (os === "mobile") heroMeta.textContent = "Available for macOS, Windows, Linux and the web";
  }

  // ================================================================ latest release
  const fmtSize = (b) => (b >= 1048576 ? `${(b / 1048576).toFixed(1)} MB` : `${Math.round(b / 1024)} KB`);
  function applyRelease(rel) {
    const assets = rel.assets || [];
    const find = (suffix) => assets.find((a) => a.name.endsWith(suffix));
    const version = (rel.tag_name || "").replace(/^v/, "");
    $$("[data-version-text]").forEach((el) => (el.textContent = el.closest(".kicker") ? `Fetchly ${version} is out` : `Fetchly ${version}`));
    $$("[data-asset]").forEach((a) => {
      const asset = find(a.getAttribute("data-asset"));
      if (!asset) return;
      a.href = asset.browser_download_url;
      const size = $(".size", a);
      if (size) size.textContent = fmtSize(asset.size);
    });
    const sums = find("SHA256SUMS.txt");
    if (sums) $("[data-sums]").href = sums.browser_download_url;
    if (info) {
      const asset = find(info.asset);
      if (asset) {
        [heroBtn, ctaBtn, dlBtn].forEach((b) => (b.href = asset.browser_download_url));
        heroMeta.textContent = `v${version} · ${info.sub} · ${fmtSize(asset.size)}`;
        $("#dl-hero-sub").textContent = `v${version} · ${fmtSize(asset.size)} · ${info.sub}`;
      }
    }
  }
  const cache = (k, v) => { try { if (v === undefined) return JSON.parse(sessionStorage.getItem(k) || "null"); sessionStorage.setItem(k, JSON.stringify(v)); } catch (_) { return null; } };
  const cachedRel = cache("fetchly-rel");
  if (cachedRel) applyRelease(cachedRel);
  fetch(`https://api.github.com/repos/${REPO}/releases/latest`)
    .then((r) => (r.ok ? r.json() : Promise.reject()))
    .then((rel) => {
      const slim = { tag_name: rel.tag_name, assets: rel.assets.map(({ name, size, browser_download_url }) => ({ name, size, browser_download_url })) };
      cache("fetchly-rel", slim);
      applyRelease(slim);
    })
    .catch(() => {});

  // GitHub stars in the nav
  const showStars = (n) => { if (n > 0) $("[data-stars]").textContent = n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n); };
  const cachedStars = cache("fetchly-stars");
  if (cachedStars) showStars(cachedStars);
  fetch(`https://api.github.com/repos/${REPO}`).then((r) => (r.ok ? r.json() : Promise.reject())).then((d) => { cache("fetchly-stars", d.stargazers_count); showStars(d.stargazers_count); }).catch(() => {});

  // ================================================================ live mini client
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  function highlight(json) {
    return esc(json).replace(/("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|([{}\[\],:])/g, (m, str, colon, kw, num, p) => {
      if (str) return colon ? `<span class="tk-k">${str}</span><span class="tk-p">${colon}</span>` : `<span class="tk-s">${str}</span>`;
      if (kw) return `<span class="tk-b">${kw}</span>`;
      if (num) return `<span class="tk-n">${num}</span>`;
      return `<span class="tk-p">${p}</span>`;
    });
  }
  const BASE = "https://jsonplaceholder.typicode.com";
  let path = "/users/1";
  const body = $("#client-body code"), sendBtn = $("#client-send"), tests = $("#client-tests");

  $$(".ep").forEach((b) => b.addEventListener("click", () => {
    $$(".ep").forEach((x) => x.classList.toggle("on", x === b));
    path = b.dataset.path;
    $("#client-path").textContent = path;
    send();
  }));

  async function send() {
    if (sendBtn.classList.contains("busy")) return;
    sendBtn.classList.add("busy");
    $$("li", tests).forEach((li) => li.classList.add("hide"));
    const t0 = performance.now();
    try {
      const res = await fetch(BASE + path, { cache: "no-store" });
      const text = await res.text();
      const ms = Math.round(performance.now() - t0);
      let data = null;
      try { data = JSON.parse(text); } catch (_) {}
      const code = $("#st-code");
      code.textContent = `${res.status} ${res.statusText || (res.ok ? "OK" : "")}`.trim();
      code.classList.toggle("err", !res.ok);
      $("#st-time").textContent = `${ms} ms`;
      $("#st-size").textContent = `${(new Blob([text]).size / 1024).toFixed(2)} KB`;
      body.innerHTML = data ? highlight(JSON.stringify(data, null, 2)) : esc(text);
      body.parentElement.classList.remove("flash"); void body.offsetWidth; body.parentElement.classList.add("flash");
      body.parentElement.scrollTop = 0;
      const results = [
        [`Status is 200`, res.status === 200],
        [`Responds in under 2 s`, ms < 2000],
        [`Has an id`, !!(data && data.id !== undefined)],
      ];
      tests.innerHTML = results.map(([name, ok]) => `<li class="${ok ? "pass" : "fail"} hide"><svg class="i"><use href="#i-${ok ? "check" : "x"}"/></svg>${name}</li>`).join("");
      $("#st-tests").innerHTML = `Tests <b style="color:${results.every((r) => r[1]) ? "var(--green)" : "var(--red)"}">${results.filter((r) => r[1]).length}/${results.length}</b>`;
      $$("li", tests).forEach((li, i) => setTimeout(() => li.classList.remove("hide"), 120 + i * 140));
    } catch (e) {
      const code = $("#st-code");
      code.textContent = "Offline";
      code.classList.add("err");
      body.innerHTML = `<span class="tk-p">// Couldn't reach the test API from here.\n// In the app, requests go out natively — no CORS, no limits.</span>`;
    } finally {
      sendBtn.classList.remove("busy");
    }
  }
  sendBtn.addEventListener("click", send);
  // Send once the client scrolls into view, so the first impression is a real response.
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver(([e]) => { if (e.isIntersecting) { io.disconnect(); setTimeout(send, 600); } }, { threshold: .5 });
    io.observe($("#client"));
  }

  // ================================================================ code tabs
  const SAMPLES = {
    curl: `<span class="tk-f">curl</span> --request POST <span class="tk-s">'https://api.example.com/v1/orders'</span> \\\n  --header <span class="tk-s">'Authorization: Bearer eyJhbGciOi…'</span> \\\n  --data-raw <span class="tk-s">'{"sku":"FX-42","qty":2}'</span>`,
    py: `<span class="tk-b">import</span> requests\n\nr = requests.<span class="tk-f">post</span>(<span class="tk-s">"https://api.example.com/v1/orders"</span>,\n    headers={<span class="tk-s">"Authorization"</span>: <span class="tk-s">"Bearer eyJhbGciOi…"</span>},\n    json={<span class="tk-s">"sku"</span>: <span class="tk-s">"FX-42"</span>, <span class="tk-s">"qty"</span>: <span class="tk-n">2</span>})`,
    js: `<span class="tk-b">const</span> res = <span class="tk-b">await</span> <span class="tk-f">fetch</span>(<span class="tk-s">"https://api.example.com/v1/orders"</span>, {\n  method: <span class="tk-s">"POST"</span>,\n  headers: { Authorization: <span class="tk-s">"Bearer eyJhbGciOi…"</span> },\n  body: JSON.<span class="tk-f">stringify</span>({ sku: <span class="tk-s">"FX-42"</span>, qty: <span class="tk-n">2</span> }),\n});`,
    rs: `<span class="tk-b">let</span> res = client\n    .<span class="tk-f">post</span>(<span class="tk-s">"https://api.example.com/v1/orders"</span>)\n    .<span class="tk-f">bearer_auth</span>(<span class="tk-s">"eyJhbGciOi…"</span>)\n    .<span class="tk-f">json</span>(&amp;json!({ <span class="tk-s">"sku"</span>: <span class="tk-s">"FX-42"</span>, <span class="tk-s">"qty"</span>: <span class="tk-n">2</span> }))\n    .<span class="tk-f">send</span>().<span class="tk-b">await</span>?;`,
  };
  const codeEl = $("#code-sample code");
  $$("#code-tabs button").forEach((b) => b.addEventListener("click", () => {
    $$("#code-tabs button").forEach((x) => x.classList.toggle("on", x === b));
    codeEl.innerHTML = SAMPLES[b.dataset.lang];
  }));

  // ================================================================ tour
  const tabs = $$(".tour-tabs button"), shots = $$(".tour-stage img");
  let tourIdx = 0, tourTimer = null, userPicked = false;
  function showShot(i) {
    tourIdx = i;
    tabs.forEach((t, k) => { t.classList.toggle("on", k === i); t.classList.remove("timing"); t.setAttribute("aria-selected", k === i); });
    shots.forEach((s, k) => s.classList.toggle("on", k === i));
    if (!userPicked && !reduced) { void tabs[i].offsetWidth; tabs[i].classList.add("timing"); }
  }
  tabs.forEach((t, i) => t.addEventListener("click", () => { userPicked = true; clearInterval(tourTimer); showShot(i); }));
  if (!reduced && "IntersectionObserver" in window) {
    new IntersectionObserver(([e]) => {
      clearInterval(tourTimer);
      if (e.isIntersecting && !userPicked) { showShot(tourIdx); tourTimer = setInterval(() => showShot((tourIdx + 1) % tabs.length), 6000); }
    }, { threshold: .35 }).observe($(".tour"));
  }

  // ================================================================ ticker (duplicate for a seamless loop)
  const track = $("#ticker");
  track.innerHTML += track.innerHTML;

  // ================================================================ nav + spotlight + reveal
  const nav = $(".nav");
  addEventListener("scroll", () => nav.classList.toggle("scrolled", scrollY > 30), { passive: true });

  $$(".tile").forEach((tile) => tile.addEventListener("pointermove", (e) => {
    const r = tile.getBoundingClientRect();
    tile.style.setProperty("--mx", `${e.clientX - r.left}px`);
    tile.style.setProperty("--my", `${e.clientY - r.top}px`);
  }));

  if (!reduced && "IntersectionObserver" in window) {
    const groups = [".section-head", ".tile", ".stat", ".vs-card", ".dl-hero", ".dl-list", ".steps li", ".faq details", ".tour", ".film", ".honest"];
    const els = $$(groups.join(","));
    els.forEach((el) => {
      const sibs = [...el.parentElement.children].filter((c) => c.matches(groups.join(",")));
      el.style.setProperty("--d", `${Math.min(sibs.indexOf(el), 5) * 70}ms`);
      el.classList.add("reveal");
    });
    const io = new IntersectionObserver((entries) => entries.forEach((e) => { if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); } }), { rootMargin: "0px 0px -8% 0px" });
    els.forEach((el) => io.observe(el));
  }

  // Pause the film when off screen.
  const video = $("#demo");
  if (video && "IntersectionObserver" in window) {
    new IntersectionObserver(([e]) => (e.isIntersecting ? video.play().catch(() => {}) : video.pause()), { threshold: .2 }).observe(video);
  }
})();
