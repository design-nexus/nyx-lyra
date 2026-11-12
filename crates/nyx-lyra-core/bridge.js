// Lyra's bridge into YouTube Music: reads the Media Session and the playing media element,
// and runs a fixed set of playback commands. It posts JSON strings to the "lyra" handler;
// the native side treats everything it receives as untrusted.
(() => {
  "use strict";
  if (window.__lyraCommand) return;
  const post = (m) => {
    try {
      window.webkit.messageHandlers.lyra.postMessage(JSON.stringify(m));
    } catch (_) {}
  };

  // The page's own Media Session handlers are what its media keys would call.
  const handlers = new Map();
  const ms = navigator.mediaSession;
  // The song's details stay here for Lyra: WebKit publishes its own media player (MPRIS)
  // once the Media Session has a title, which would show Lyra twice; Lyra has its own.
  let metadata = null;
  if (ms && "metadata" in ms) {
    try {
      Object.defineProperty(ms, "metadata", { configurable: true, get: () => metadata, set: (m) => (metadata = m) });
    } catch (_) {}
  }
  if (ms && typeof ms.setActionHandler === "function") {
    const set = ms.setActionHandler.bind(ms);
    ms.setActionHandler = function (action, fn) {
      if (fn) handlers.set(action, fn);
      else handlers.delete(action);
      return set(action, fn);
    };
  }

  const media = () => {
    let best = null;
    for (const m of document.querySelectorAll("video, audio")) {
      if (!m.currentSrc) continue;
      if (!m.paused) return m;
      best = best || m;
    }
    return best;
  };
  const cookie = (n) => (document.cookie.match(new RegExp("(?:^|; )" + n + "=([^;]*)")) || [])[1];
  const num = (x) => (typeof x === "number" && isFinite(x) ? Math.round(x * 1000) / 1000 : null);
  // YouTube Music's own player bar (the app's, not the one in its page header) and the
  // queue's Autoplay switch: like, shuffle, repeat and Autoplay exist only there.
  const bar = () => document.querySelector("ytmusic-player-bar.ytmusic-app") || document.querySelector("ytmusic-player-bar");
  const likeBox = () => { const b = bar(); return b ? b.querySelector("ytmusic-like-button-renderer") : null; };
  const autoplay = () => document.querySelector("tp-yt-paper-toggle-button#automix");
  const press = (el) => { if (!el) return false; el.click(); return true; };
  const snapshot = () => {
    const m = media();
    const md = ms ? ms.metadata : null;
    const art = md && md.artwork && md.artwork.length ? md.artwork[md.artwork.length - 1].src : null;
    return {
      kind: "state",
      title: md ? md.title : null,
      artist: md ? md.artist : null,
      album: md ? md.album : null,
      artwork: art,
      session: ms ? ms.playbackState : null,
      media: !!m,
      paused: m ? m.paused : null,
      ended: m ? m.ended : null,
      position: m ? num(m.currentTime) : null,
      duration: m ? num(m.duration) : null,
      volume: m ? num(m.volume) : null,
      muted: m ? m.muted : null,
      ready: m ? m.readyState : null,
      error: m && m.error ? m.error.code : null,
      actions: [...handlers.keys()].sort(),
      path: location.pathname,
      video: new URLSearchParams(location.search).get("v"),
      list: new URLSearchParams(location.search).get("list"),
      signed: !!(cookie("SAPISID") || cookie("__Secure-3PAPISID")),
      like: (likeBox() && likeBox().getAttribute("like-status")) || null,
      repeat: (bar() && bar().getAttribute("repeat-mode")) || null,
      autoplay: autoplay() ? autoplay().hasAttribute("checked") : null,
    };
  };

  let last = "";
  let lastPos = -1;
  const send = () => {
    const s = snapshot();
    const key = JSON.stringify({ ...s, position: null });
    if (key !== last || (s.position !== null && Math.abs(s.position - lastPos) >= 5)) {
      last = key;
      lastPos = s.position ?? -1;
      post(s);
    }
  };
  for (const ev of ["play", "playing", "pause", "waiting", "seeked", "ended", "durationchange", "volumechange", "emptied", "error", "loadedmetadata", "stalled"])
    document.addEventListener(ev, send, true);
  setInterval(send, 1000);

  const act = (name, details) => {
    const fn = handlers.get(name);
    if (!fn) return false;
    fn({ action: name, ...details });
    return true;
  };
  const command = (c) => {
    const m = media();
    switch (c && c.cmd) {
      case "state":
        return snapshot();
      case "play":
        if (act("play")) return "session";
        if (m) return m.play(), "element";
        return "no-media";
      case "pause":
        if (act("pause")) return "session";
        if (m) return m.pause(), "element";
        return "no-media";
      case "play-element":
        if (!m) return "no-media";
        return m.play().then(() => "played", (e) => "refused: " + e.name), "element";
      case "toggle":
        return command({ cmd: m && !m.paused ? "pause" : "play" });
      case "next":
        return act("nexttrack") ? "session" : "unsupported";
      case "previous":
        return act("previoustrack") ? "session" : "unsupported";
      case "seek": {
        const t = Number(c.position);
        if (!isFinite(t) || t < 0) return "bad-argument";
        if (act("seekto", { seekTime: t })) return "session";
        if (m) return (m.currentTime = t), "element";
        return "no-media";
      }
      case "like": {
        const box = likeBox();
        const button = box && box.querySelectorAll("button")[0];
        if (!button) return "unsupported";
        if ((box.getAttribute("like-status") === "LIKE") !== !!c.on) button.click();
        return "element";
      }
      case "shuffle": {
        const b = bar();
        return b && press(b.querySelector("yt-icon-button.shuffle button")) ? "element" : "unsupported";
      }
      case "repeat": {
        const order = ["NONE", "ALL", "ONE"];
        const b = bar();
        const want = order.indexOf(c.mode);
        const button = b && b.querySelector("yt-icon-button.repeat button");
        if (want < 0) return "bad-argument";
        if (!button) return "unsupported";
        // The button goes NONE, ALL, ONE; press it as often as it takes.
        const steps = (want - order.indexOf(b.getAttribute("repeat-mode")) + 3) % 3;
        for (let i = 0; i < steps; i++) button.click();
        return "element";
      }
      case "autoplay": {
        const a = autoplay();
        if (!a) return "unsupported";
        if (a.hasAttribute("checked") !== !!c.on) a.click();
        return "element";
      }
      case "volume": {
        const v = Number(c.level);
        if (!isFinite(v) || v < 0 || v > 100) return "bad-argument";
        if (!m) return "no-media";
        m.volume = v / 100;
        return "element";
      }
      default:
        return "unknown";
    }
  };
  window.__lyraCommand = (c) => JSON.stringify(command(c));

  // The web client's own data requests, made from this page the way it makes them: its
  // context from ytcfg, and its signed-in header unless auth is false. Only these endpoints.
  const ENDPOINTS = new Set(["search", "browse", "next", "account/account_menu", "account/accounts_list", "playlist/create", "subscription/subscribe", "subscription/unsubscribe", "like/like", "like/removelike"]);
  const cfg = (k) => (window.ytcfg && window.ytcfg.get ? window.ytcfg.get(k) : undefined);
  const sha1 = async (s) =>
    [...new Uint8Array(await crypto.subtle.digest("SHA-1", new TextEncoder().encode(s)))].map((b) => b.toString(16).padStart(2, "0")).join("");
  const signedIn = async () => {
    const sid = cookie("SAPISID") || cookie("__Secure-3PAPISID");
    if (!sid) return null;
    const t = Math.floor(Date.now() / 1000);
    return `SAPISIDHASH ${t}_${await sha1(`${t} ${sid} ${location.origin}`)}`;
  };
  window.__lyraData = async (c) => {
    if (!c || !ENDPOINTS.has(c.endpoint) || typeof c.body !== "object") return JSON.stringify({ error: "bad-request" });
    const context = cfg("INNERTUBE_CONTEXT");
    if (!context) return JSON.stringify({ error: "no-client-context" });
    const headers = {
      "Content-Type": "application/json",
      "X-Goog-AuthUser": String(cfg("SESSION_INDEX") ?? 0),
      "X-Origin": location.origin,
      "X-Youtube-Client-Name": String(cfg("INNERTUBE_CONTEXT_CLIENT_NAME") ?? ""),
      "X-Youtube-Client-Version": String(cfg("INNERTUBE_CLIENT_VERSION") ?? ""),
    };
    // A channel (brand account) chosen at sign-in: without its page ID the requests reach
    // the Google account behind it, as YouTube Music's own client would without it.
    const page = cfg("DELEGATED_SESSION_ID");
    if (page) headers["X-Goog-PageId"] = String(page);
    const visitor = cfg("VISITOR_DATA");
    if (visitor) headers["X-Goog-Visitor-Id"] = String(visitor);
    if (c.auth !== false) {
      const a = await signedIn();
      if (a) headers.Authorization = a;
    }
    const t0 = performance.now();
    const r = await fetch(`/youtubei/v1/${c.endpoint}?prettyPrint=false`, { method: "POST", credentials: "include", headers, body: JSON.stringify({ context, ...c.body }) });
    const text = await r.text();
    return JSON.stringify({ status: r.status, ms: Math.round(performance.now() - t0), bytes: text.length, signed: !!headers.Authorization, channel: !!page, text });
  };
  post({ kind: "hello", path: location.pathname });
})();
