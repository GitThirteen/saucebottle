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
import { Trash2Icon, ChevronDownIcon } from 'lucide-vue-next';
import {
  logEntries, logger,
  activeFilters, toggleFilter, selectAll, isAllActive, isLevelActive,
  type LogLevel
} from '../logger';

// ── Filter definitions ───────────────────────────────────────────────────────
const levels: { label: string; value: LogLevel }[] = [
  { label: 'Info',  value: 'info'    },
  { label: 'Done',  value: 'success' },
  { label: 'Warn',  value: 'warn'    },
  { label: 'Error', value: 'error'   },
];

// Filtered view — reacts to both logEntries and activeFilters
const visible = computed(() => {
  // Access activeFilters.value to establish reactivity on Set replacement
  const filters = activeFilters.value;
  if (filters.has('all')) return logEntries.value;
  return logEntries.value.filter(e => filters.has(e.level));
});

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
  info:    'INFO',
  warn:    'WARN',
  error:   'ERROR',
  success: 'DONE',
};

function fmt(d: Date) {
  return d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}
</script>

<template>
  <div class="logs-view">

    <!-- Toolbar -->
    <div class="toolbar unselectable">
      <div class="filter-tabs">

        <!-- "All" button -->
        <button
          class="filter-btn filter-all"
          :class="{ active: isAllActive() }"
          @click="selectAll()"
        >All</button>

        <!-- Individual level toggles -->
        <button
          v-for="lvl in levels"
          :key="lvl.value"
          class="filter-btn"
          :class="[`filter-${lvl.value}`, { active: isLevelActive(lvl.value) && !isAllActive() }]"
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
        <div
          v-for="entry in visible"
          :key="entry.id"
          class="log-row"
          :class="`log-${entry.level}`"
        >
          <span class="ts">{{ fmt(entry.timestamp) }}</span>
          <span class="tag" :class="`tag-${entry.level}`">{{ tagLabel[entry.level] }}</span>
          <span class="msg">{{ entry.message }}</span>
        </div>
      </transition-group>

      <div v-if="visible.length === 0" class="empty-state unselectable">
        <span class="empty-icon">⬛</span>
        <p>No log entries yet.</p>
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

/* All active */
.filter-all.active { background: rgba(255,255,255,0.1); border-color: rgba(255,255,255,0.2); color: var(--text-primary); }

/* Individual level active states */
.filter-info.active    { background: rgba(96,165,250,0.2);  border-color: rgba(96,165,250,0.5);  color: #60a5fa; }
.filter-success.active { background: rgba(0,230,118,0.15);  border-color: rgba(0,230,118,0.4);   color: #00e676; }
.filter-warn.active    { background: rgba(245,197,66,0.15); border-color: rgba(245,197,66,0.4);  color: #f5c542; }
.filter-error.active   { background: rgba(255,77,77,0.15);  border-color: rgba(255,77,77,0.4);   color: #ff4d4d; }

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

.log-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 3px 12px;
  font-size: 0.78rem;
  line-height: 1.6;
  border-left: 2px solid transparent;
  transition: background 0.1s;
}
.log-row:hover { background: rgba(255,255,255,0.03); }

.log-info    { border-left-color: rgba(96,165,250,0.35);  }
.log-success { border-left-color: rgba(0,230,118,0.35);   }
.log-warn    { border-left-color: rgba(245,197,66,0.35);  }
.log-error   { border-left-color: rgba(255,77,77,0.45);   }

.ts {
  color: var(--text-tertiary);
  font-size: 0.68rem;
  flex-shrink: 0;
  opacity: 0.6;
}

.tag {
  flex-shrink: 0;
  font-size: 0.68rem;
  font-weight: 800;
  padding: 1px 5px;
  border-radius: 4px;
  letter-spacing: 0.05em;
}
.tag-info    { background: rgba(96,165,250,0.15);  color: #60a5fa; }
.tag-success { background: rgba(0,230,118,0.12);   color: #00e676; }
.tag-warn    { background: rgba(245,197,66,0.15);  color: #f5c542; }
.tag-error   { background: rgba(255,77,77,0.15);   color: #ff4d4d; }

.msg { color: var(--text-secondary); word-break: break-word; flex: 1; }
.log-error .msg   { color: #fca5a5; }
.log-success .msg { color: #86efac; }
.log-warn .msg    { color: #fde68a; }

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