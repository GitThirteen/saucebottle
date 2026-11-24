<!--
SauceBottle - An anime artwork sorter daemon written in Tauri & Rust.
Copyright © 2026    Thirteen

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.
-->

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted } from 'vue';
import { Trash2Icon, ChevronDownIcon, ChevronRightIcon } from 'lucide-vue-next';
import {
  logEntries, logger,
  activeFilters, toggleFilter, isLevelActive,
  type LogEntry, type LogLevel
} from '../logger';

// ── Filter definitions ───────────────────────────────────────────────────────
// Every level is its own on/off switch. Debug is off by default (see logger.ts).
const levels: { label: string; value: LogLevel }[] = [
  { label: 'Info',  value: 'info'    },
  { label: 'Done',  value: 'success' },
  { label: 'Warn',  value: 'warn'    },
  { label: 'Error', value: 'error'   },
  { label: 'Debug', value: 'debug'   },
];

// Filtered view — reacts to both logEntries and activeFilters
const visible = computed(() => {
  // Access activeFilters.value to establish reactivity on Set replacement
  const filters = activeFilters.value;
  return logEntries.value.filter(e => filters.has(e.level));
});

// ── Time ─────────────────────────────────────────────────────────────────────
// There's no time column. A thin divider marks each new minute, and the exact time of a row
// is in its tooltip.

function minuteLabel(d: Date) {
  const time = d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === new Date().toDateString()) return time;
  return `${d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short' })} ${time}`;
}

/** Entries that start a new minute, with the label for their divider. */
const dividers = computed(() => {
  const labels = new Map<number, string>();
  let lastMinute = '';

  for (const entry of visible.value) {
    const label = minuteLabel(entry.timestamp);
    if (label !== lastMinute) {
      labels.set(entry.id, label);
      lastMinute = label;
    }
  }
  return labels;
});

function fmtExact(d: Date) {
  return d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function timeTitle(entry: LogEntry) {
  if (entry.count > 1) return `${entry.count} times, first at ${fmtExact(entry.timestamp)}, last at ${fmtExact(entry.lastSeen)}`;
  return fmtExact(entry.timestamp);
}

// ── Expandable rows ──────────────────────────────────────────────────────────
const expanded = ref<Set<number>>(new Set());
const copiedId = ref<number | null>(null);
let copiedTimeout: number | undefined;

const hasDetails = (entry: LogEntry) => !!entry.details?.length;
const isOpen = (entry: LogEntry) => expanded.value.has(entry.id);

function toggleRow(entry: LogEntry) {
  if (!hasDetails(entry)) return;
  if (window.getSelection()?.toString()) return; // Selecting text to copy it shouldn't fold the row

  const next = new Set(expanded.value);
  if (!next.delete(entry.id)) next.add(entry.id);
  expanded.value = next;
}

async function copyEntry(entry: LogEntry) {
  const lines = [entry.message, ...(entry.details ?? []).map(d => `${d.label}: ${d.value}`)];

  try {
    await navigator.clipboard.writeText(lines.join('\n'));
    copiedId.value = entry.id;
    clearTimeout(copiedTimeout);
    copiedTimeout = window.setTimeout(() => { copiedId.value = null; }, 1500);
  } catch (e) {
    console.error("Failed to copy log entry:", e);
  }
}

// ── Auto-scroll ──────────────────────────────────────────────────────────────
const scrollEl = ref<HTMLElement | null>(null);
const isAtBottom = ref(true);

function checkScroll() {
  if (!scrollEl.value) return;
  const { scrollTop, scrollHeight, clientHeight } = scrollEl.value;
  isAtBottom.value = scrollHeight - scrollTop - clientHeight < 32;
}

function scrollToBottom() {
  nextTick(() => {
    if (scrollEl.value) scrollEl.value.scrollTop = scrollEl.value.scrollHeight;
  });
}

watch(visible, () => { if (isAtBottom.value) scrollToBottom(); }, { flush: 'post' });
onMounted(scrollToBottom);

// ── Helpers ──────────────────────────────────────────────────────────────────
const tagLabel: Record<LogLevel, string> = {
  debug:   'DEBUG',
  info:    'INFO',
  warn:    'WARN',
  error:   'ERROR',
  success: 'DONE',
};

const emptyText = computed(() =>
  logEntries.value.length === 0 ? 'No log entries yet.' : 'Nothing to show for the selected levels.'
);
</script>

<template>
  <div class="logs-view">

    <!-- Toolbar -->
    <div class="toolbar unselectable">
      <div class="filter-tabs">

        <!-- One toggle per level -->
        <button
          v-for="lvl in levels"
          :key="lvl.value"
          class="filter-btn"
          :class="[`filter-${lvl.value}`, { active: isLevelActive(lvl.value) }]"
          :aria-pressed="isLevelActive(lvl.value)"
          @click="toggleFilter(lvl.value)"
        >{{ lvl.label }}</button>

      </div>

      <button class="clear-btn" @click="logger.clear()" title="Clear logs">
        <Trash2Icon :size="14" />
        Clear
      </button>
    </div>

    <!-- Log output -->
    <div class="console-body" ref="scrollEl" @scroll="checkScroll">
      <transition-group name="log-row" tag="div">
        <div v-for="entry in visible" :key="entry.id" class="log-item">
          <div v-if="dividers.has(entry.id)" class="time-divider unselectable">{{ dividers.get(entry.id) }}</div>

          <div
            class="log-row"
            :class="[`log-${entry.level}`, { expandable: hasDetails(entry), open: isOpen(entry) }]"
            :title="timeTitle(entry)"
            :tabindex="hasDetails(entry) ? 0 : undefined"
            @click="toggleRow(entry)"
            @keydown.enter="toggleRow(entry)"
          >
            <span class="tag" :class="`tag-${entry.level}`">{{ tagLabel[entry.level] }}</span>
            <span class="msg">{{ entry.message }}</span>
            <span v-if="entry.count > 1" class="count unselectable">×{{ entry.count }}</span>
            <ChevronRightIcon v-if="hasDetails(entry)" :size="12" class="caret unselectable" :class="{ open: isOpen(entry) }" />

            <div v-if="isOpen(entry)" class="details" @click.stop>
              <div v-for="detail in entry.details" :key="detail.label" class="detail">
                <span class="detail-label unselectable">{{ detail.label }}</span>
                <span class="detail-value">{{ detail.value }}</span>
              </div>
              <button class="copy-btn unselectable" @click="copyEntry(entry)">
                {{ copiedId === entry.id ? 'Copied' : 'Copy' }}
              </button>
            </div>
          </div>
        </div>
      </transition-group>

      <div v-if="visible.length === 0" class="empty-state unselectable">
        <span class="empty-icon">⬛</span>
        <p>{{ emptyText }}</p>
      </div>
    </div>

    <!-- Scroll-to-bottom -->
    <transition name="fab">
      <button v-if="!isAtBottom" class="scroll-fab unselectable" @click="scrollToBottom">
        <ChevronDownIcon :size="18" />
      </button>
    </transition>

  </div>
</template>

<style scoped>
.logs-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-base);
  font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', ui-monospace, monospace;
  position: relative;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--bg-surface);
  border-bottom: 1px solid var(--bg-surface-elevated);
  gap: 8px;
  flex-shrink: 0;
}

.filter-tabs { display: flex; gap: 4px; flex-wrap: wrap; }

.filter-btn {
  padding: 4px 10px;
  border-radius: 999px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 0.7rem;
  font-weight: 700;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;
  letter-spacing: 0.04em;
  user-select: none;
}
.filter-btn:hover { color: var(--text-primary); background: var(--bg-surface-elevated); }

/* Level active states */
.filter-info.active    { background: rgba(96,165,250,0.2);  border-color: rgba(96,165,250,0.5);  color: #60a5fa; }
.filter-success.active { background: rgba(0,230,118,0.15);  border-color: rgba(0,230,118,0.4);   color: #00e676; }
.filter-warn.active    { background: rgba(245,197,66,0.15); border-color: rgba(245,197,66,0.4);  color: #f5c542; }
.filter-error.active   { background: rgba(255,77,77,0.15);  border-color: rgba(255,77,77,0.4);   color: #ff4d4d; }
.filter-debug.active   { background: rgba(148,163,184,0.15); border-color: rgba(148,163,184,0.4); color: #94a3b8; }

.clear-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  border-radius: 999px;
  border: 1px solid var(--bg-surface-elevated);
  background: transparent;
  color: var(--text-tertiary);
  font-size: 0.7rem;
  font-weight: 700;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;
  flex-shrink: 0;
}
.clear-btn:hover { color: #ff4d4d; border-color: rgba(255,77,77,0.4); background: rgba(255,77,77,0.08); }

.console-body {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
  scroll-behavior: smooth;
}
.console-body::-webkit-scrollbar { width: 6px; }
.console-body::-webkit-scrollbar-track { background: transparent; }
.console-body::-webkit-scrollbar-thumb { background: var(--bg-surface-elevated); border-radius: 3px; }

/* Marks the start of a new minute */
.time-divider {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px 2px;
  font-size: 0.65rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-tertiary);
  opacity: 0.7;
}
.time-divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--bg-surface-elevated);
}

.log-row {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 8px;
  padding: 3px 12px;
  font-size: 0.78rem;
  line-height: 1.6;
  border-left: 2px solid transparent;
  transition: background 0.1s;
}
.log-row:hover { background: rgba(255,255,255,0.03); }
.log-row.expandable { cursor: pointer; }
.log-row.open { background: rgba(255,255,255,0.04); }
.log-row:focus-visible { outline: 1px solid var(--bg-surface-elevated); outline-offset: -1px; }

.log-debug   { border-left-color: rgba(148,163,184,0.3);  }
.log-info    { border-left-color: rgba(96,165,250,0.35);  }
.log-success { border-left-color: rgba(0,230,118,0.35);   }
.log-warn    { border-left-color: rgba(245,197,66,0.35);  }
.log-error   { border-left-color: rgba(255,77,77,0.45);   }

.tag {
  flex-shrink: 0;
  font-size: 0.68rem;
  font-weight: 800;
  padding: 1px 5px;
  border-radius: 4px;
  letter-spacing: 0.05em;
}
.tag-debug   { background: rgba(148,163,184,0.12); color: #94a3b8; }
.tag-info    { background: rgba(96,165,250,0.15);  color: #60a5fa; }
.tag-success { background: rgba(0,230,118,0.12);   color: #00e676; }
.tag-warn    { background: rgba(245,197,66,0.15);  color: #f5c542; }
.tag-error   { background: rgba(255,77,77,0.15);   color: #ff4d4d; }

.msg { color: var(--text-secondary); word-break: break-word; flex: 1; min-width: 0; }
.log-debug .msg   { color: var(--text-tertiary); }
.log-error .msg   { color: #fca5a5; }
.log-success .msg { color: #86efac; }
.log-warn .msg    { color: #fde68a; }

/* "×3": the same message came in several times */
.count {
  flex-shrink: 0;
  font-size: 0.68rem;
  font-weight: 800;
  padding: 0 6px;
  border-radius: 999px;
  color: var(--text-secondary);
  background: rgba(255,255,255,0.08);
}

.caret {
  flex-shrink: 0;
  align-self: center;
  color: var(--text-tertiary);
  transition: transform 0.15s;
}
.caret.open { transform: rotate(90deg); }

/* Details of an expanded row */
.details {
  flex-basis: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 2px 0 4px;
  padding: 6px 10px;
  border-radius: 6px;
  background: rgba(0,0,0,0.2);
  cursor: default;
  font-size: 0.72rem;
  line-height: 1.5;
}
.detail { display: flex; gap: 10px; align-items: baseline; }
.detail-label { flex: 0 0 64px; color: var(--text-tertiary); font-weight: 700; }
.detail-value { flex: 1; min-width: 0; color: var(--text-secondary); word-break: break-word; }

.copy-btn {
  align-self: flex-end;
  margin-top: 4px;
  padding: 2px 10px;
  border-radius: 999px;
  border: 1px solid var(--bg-surface-elevated);
  background: transparent;
  color: var(--text-tertiary);
  font-size: 0.68rem;
  font-weight: 700;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;
}
.copy-btn:hover { color: var(--text-primary); border-color: var(--text-tertiary); }

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 60px 20px;
  color: var(--text-tertiary);
  font-family: sans-serif;
}
.empty-icon { font-size: 2rem; opacity: 0.3; }
.empty-state p { margin: 0; font-size: 0.85rem; opacity: 0.5; }

.scroll-fab {
  position: absolute;
  bottom: 16px;
  right: 16px;
  width: 34px;
  height: 34px;
  border-radius: 50%;
  background: var(--bg-surface);
  border: 1px solid var(--bg-surface-elevated);
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 4px 14px rgba(0,0,0,0.4);
  transition: transform 0.15s, background 0.15s;
}
.scroll-fab:hover { transform: scale(1.1); background: var(--bg-surface-elevated); }

.log-row-enter-active { transition: opacity 0.2s, transform 0.2s; }
.log-row-enter-from   { opacity: 0; transform: translateX(-6px); }
.log-row-leave-active { display: none; }

.fab-enter-active, .fab-leave-active { transition: opacity 0.2s, transform 0.2s; }
.fab-enter-from, .fab-leave-to       { opacity: 0; transform: scale(0.8); }
</style>