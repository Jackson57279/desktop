const urlInput = document.querySelector("#url");
const spacesEl = document.querySelector("#spaces");
const spaceNameEl = document.querySelector("#space-name");
const pinnedEl = document.querySelector("#pinned");
const tabsEl = document.querySelector("#tabs");

let lastKey = "";

async function post(path, body) {
  await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  lastKey = "";
  await refresh();
}

function letter(url) {
  try {
    const host = new URL(url).hostname.replace(/^www\./, "");
    return (host[0] || "•").toUpperCase();
  } catch {
    return "•";
  }
}

function tabRow(tab) {
  const row = document.createElement("div");
  row.className = tab.active ? "tab active" : "tab";
  const glyph = document.createElement("span");
  glyph.className = "glyph";
  glyph.textContent = letter(tab.url);
  const title = document.createElement("span");
  title.className = "title";
  title.textContent = tab.title || "New Tab";
  const pin = document.createElement("button");
  pin.className = "icon-button";
  pin.type = "button";
  pin.textContent = tab.pinned ? "unpin" : "pin";
  pin.addEventListener("click", (event) => {
    event.stopPropagation();
    post("/api/pin", { tabId: tab.id, pinned: !tab.pinned });
  });
  const close = document.createElement("button");
  close.className = "icon-button";
  close.type = "button";
  close.textContent = "×";
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    post("/api/close", { tabId: tab.id });
  });
  row.append(glyph, title, pin, close);
  row.addEventListener("click", () => post("/api/activate", { tabId: tab.id }));
  return row;
}

function render(state) {
  document.body.classList.toggle("compact", state.compact);
  const active = state.spaces.find((space) => space.id === state.activeSpaceId);
  spaceNameEl.textContent = active ? active.name : "Space";
  spacesEl.replaceChildren();
  for (const space of state.spaces) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = space.id === state.activeSpaceId ? "space active" : "space";
    button.title = space.name;
    const dot = document.createElement("span");
    dot.className = "dot";
    dot.style.background = space.color;
    button.append(dot);
    button.addEventListener("click", () => post("/api/activate-space", { spaceId: space.id }));
    button.addEventListener("dblclick", () => {
      const name = window.prompt("Rename space", space.name);
      if (name) post("/api/rename-space", { spaceId: space.id, name });
    });
    spacesEl.append(button);
  }
  const visible = state.tabs.filter((tab) => tab.spaceId === state.activeSpaceId);
  pinnedEl.replaceChildren(...visible.filter((tab) => tab.pinned).map(tabRow));
  tabsEl.replaceChildren(...visible.filter((tab) => !tab.pinned).map(tabRow));
  const current = visible.find((tab) => tab.active);
  if (document.activeElement !== urlInput) {
    urlInput.value = current && current.url !== "about:blank" ? current.url : "";
  }
}

async function refresh() {
  const state = await fetch("/api/state").then((response) => response.json());
  const key = JSON.stringify(state);
  if (key === lastKey) return;
  lastKey = key;
  render(state);
}

document.querySelector("#urlbar").addEventListener("submit", (event) => {
  event.preventDefault();
  post("/api/navigate", { url: urlInput.value });
});
document.querySelector("#new-tab").addEventListener("click", () => post("/api/new-tab", {}));
document.querySelector("#new-space").addEventListener("click", () => {
  const name = window.prompt("New space", "Space");
  if (name) post("/api/new-space", { name });
});
document.querySelector("#compact").addEventListener("click", () => post("/api/compact", {}));
document.querySelector("#side").addEventListener("click", () => post("/api/right-side", {}));

refresh();
setInterval(() => refresh().catch(() => {}), 400);
