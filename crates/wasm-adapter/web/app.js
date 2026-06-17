// SPDX-License-Identifier: MPL-1.1
import init, { AvroState } from "../pkg/wasm_adapter.js";

const DRAFTS_KEY = "avro-typing-pad:drafts";

const canvasEl = document.getElementById("canvas");
const suggestionsEl = document.getElementById("suggestions");
const langToggleEl = document.getElementById("lang-toggle");
const clearBtn = document.getElementById("clear-btn");
const copyBtn = document.getElementById("copy-btn");
const saveDraftBtn = document.getElementById("save-draft-btn");
const dictStatusEl = document.getElementById("dict-status");
const draftsListEl = document.getElementById("drafts-list");

let state = null;
let committed = "";
let preedit = "";
let suggestions = [];
let banglaMode = true;

function render() {
  canvasEl.textContent = "";
  const committedNode = document.createTextNode(committed);
  canvasEl.appendChild(committedNode);
  if (preedit) {
    const preeditSpan = document.createElement("span");
    preeditSpan.className = "preedit";
    preeditSpan.textContent = preedit;
    canvasEl.appendChild(preeditSpan);
  }
  canvasEl.classList.toggle("is-empty", committed.length === 0 && preedit.length === 0);

  suggestionsEl.textContent = "";
  suggestions.forEach((word, i) => {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "chip";
    chip.dataset.index = String(i);
    const idx = document.createElement("span");
    idx.className = "chip-index";
    idx.textContent = String(i + 1);
    chip.appendChild(idx);
    chip.appendChild(document.createTextNode(word));
    chip.addEventListener("click", () => applySuggestion(i));
    suggestionsEl.appendChild(chip);
  });
}

function clearPreeditAndSuggestions() {
  preedit = "";
  suggestions = [];
}

function applySuggestion(index) {
  if (!state || index < 0 || index >= suggestions.length) return;
  const word = state.commit_suggestion(index);
  committed += word;
  clearPreeditAndSuggestions();
  render();
}

function handleKeydown(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) return;

  if (e.key === "Backspace") {
    e.preventDefault();
    if (banglaMode && state) {
      preedit = state.handle_backspace();
      suggestions = state.suggestions();
    } else if (committed.length > 0) {
      committed = committed.slice(0, -1);
    }
    render();
    return;
  }

  if (e.key === "Enter") {
    e.preventDefault();
    if (banglaMode && state && state.has_preedit()) {
      committed += state.commit();
      clearPreeditAndSuggestions();
    }
    render();
    return;
  }

  if (e.key === " ") {
    e.preventDefault();
    if (banglaMode && state && state.has_preedit()) {
      committed += state.commit();
      clearPreeditAndSuggestions();
    }
    committed += " ";
    render();
    return;
  }

  if (/^[1-5]$/.test(e.key) && banglaMode && state && state.has_preedit()) {
    e.preventDefault();
    applySuggestion(Number(e.key) - 1);
    return;
  }

  if (e.key.length === 1 && e.key.charCodeAt(0) >= 32 && e.key.charCodeAt(0) < 127) {
    e.preventDefault();
    if (banglaMode && state) {
      preedit = state.handle_input(e.key);
      suggestions = state.suggestions();
    } else {
      committed += e.key;
    }
    render();
  }
}

function clearCanvas() {
  committed = "";
  clearPreeditAndSuggestions();
  render();
}

async function copyCanvas() {
  try {
    await navigator.clipboard.writeText(committed);
  } catch {
    // Clipboard access denied or unavailable; nothing else to do.
  }
}

function loadDrafts() {
  try {
    return JSON.parse(localStorage.getItem(DRAFTS_KEY) || "[]");
  } catch {
    return [];
  }
}

function saveDrafts(drafts) {
  localStorage.setItem(DRAFTS_KEY, JSON.stringify(drafts));
}

function renderDrafts() {
  const drafts = loadDrafts();
  draftsListEl.textContent = "";
  drafts.forEach((draft) => {
    const li = document.createElement("li");
    li.className = "draft-item";

    const title = document.createElement("span");
    title.className = "draft-title";
    title.textContent = draft.title;
    li.appendChild(title);

    const del = document.createElement("button");
    del.type = "button";
    del.className = "draft-delete";
    del.textContent = "✕";
    del.addEventListener("click", (ev) => {
      ev.stopPropagation();
      deleteDraft(draft.id);
    });
    li.appendChild(del);

    li.addEventListener("click", () => {
      committed = draft.text;
      clearPreeditAndSuggestions();
      render();
    });

    draftsListEl.appendChild(li);
  });
}

function saveCurrentDraft() {
  if (!committed.trim()) return;
  const drafts = loadDrafts();
  const title = committed.trim().slice(0, 24) || "Untitled";
  drafts.unshift({
    id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6),
    title,
    text: committed,
    savedAt: new Date().toISOString(),
  });
  saveDrafts(drafts);
  renderDrafts();
}

function deleteDraft(id) {
  const drafts = loadDrafts().filter((d) => d.id !== id);
  saveDrafts(drafts);
  renderDrafts();
}

function setDictStatus(text, hide) {
  dictStatusEl.textContent = text;
  dictStatusEl.style.display = hide ? "none" : "";
}

async function loadDictionariesInBackground() {
  let dictText = null;
  let suffixText = null;
  try {
    const [dictRes, suffixRes] = await Promise.all([
      fetch("./data/avrodict.js"),
      fetch("./data/suffixdict.js"),
    ]);
    [dictText, suffixText] = await Promise.all([dictRes.text(), suffixRes.text()]);
  } catch {
    setDictStatus("Suggestions unavailable", true);
    return;
  }

  try {
    if (dictText) state.load_dict(dictText);
    if (suffixText) state.load_suffix_dict(suffixText);
    setDictStatus("Suggestions ready", true);
  } catch {
    setDictStatus("Suggestions unavailable", true);
  }
}

async function main() {
  await init();

  let grammarJson = null;
  try {
    const res = await fetch("./data/avro.json");
    grammarJson = await res.text();
  } catch {
    grammarJson = null;
  }

  state = new AvroState(grammarJson, null, null);

  canvasEl.addEventListener("keydown", handleKeydown);
  canvasEl.addEventListener("click", () => canvasEl.focus());
  canvasEl.focus();

  langToggleEl.addEventListener("change", () => {
    banglaMode = langToggleEl.checked;
  });

  clearBtn.addEventListener("click", clearCanvas);
  copyBtn.addEventListener("click", copyCanvas);
  saveDraftBtn.addEventListener("click", saveCurrentDraft);

  renderDrafts();
  render();

  loadDictionariesInBackground();
}

main();
