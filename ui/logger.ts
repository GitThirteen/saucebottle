/**
 * SauceBottle - An anime artwork sorter daemon written in Tauri & Rust.
 * Copyright © 2026    Thirteen
 * 
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * 
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 * 
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { ref, watch } from 'vue';

export type LogLevel = 'info' | 'warn' | 'error' | 'success';

export interface LogEntry {
  id: number;
  level: LogLevel;
  message: string;
  timestamp: Date;
}

// ── Persistence ──────────────────────────────────────────────────────────────
const STORAGE_KEY = 'saucebottle_log_filters';

function loadFilters(): Set<LogLevel | 'all'> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed: string[] = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.length > 0) {
        return new Set(parsed as (LogLevel | 'all')[]);
      }
    }
  } catch { }
  return new Set(['all']);
}

function saveFilters(filters: Set<LogLevel | 'all'>) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...filters]));
  } catch { }
}

// ── Core state ───────────────────────────────────────────────────────────────
let _nextId = 0;
const MAX_ENTRIES = 500;

export const logEntries = ref<LogEntry[]>([]);
export const unreadCount = ref(0);

// Stored as a ref<Set> — Vue can't deeply observe Sets natively (as far as I know), so we
// replace the whole Set on each mutation to trigger reactivity.
export const activeFilters = ref<Set<LogLevel | 'all'>>(loadFilters());

watch(
  activeFilters,
  (val) => saveFilters(val),
  { deep: true }
);

// ── Filter helpers (used by Logs.vue and the push function) ──────────────────

export function toggleFilter(level: LogLevel) {
  const next = new Set(activeFilters.value);
  next.delete('all');

  if (next.has(level)) {
    next.delete(level);
    if (next.size === 0) next.add('all');
  } else {
    next.add(level);
  }

  activeFilters.value = next;
}

export function selectAll() {
  activeFilters.value = new Set(['all']);
}

export function isAllActive(): boolean {
  return activeFilters.value.has('all');
}

export function isLevelActive(level: LogLevel): boolean {
  return activeFilters.value.has(level);
}

// ── Tab tracking ─────────────────────────────────────────────────────────────
let _isLogTabOpen = false;

export const setLogTabOpen = (open: boolean) => {
  _isLogTabOpen = open;
  if (open) unreadCount.value = 0;
};

// ── Push ─────────────────────────────────────────────────────────────────────
function push(level: LogLevel, message: string) {
  logEntries.value.push({ id: _nextId++, level, message, timestamp: new Date() });
  if (logEntries.value.length > MAX_ENTRIES) {
    logEntries.value.splice(0, logEntries.value.length - MAX_ENTRIES);
  }

  // Badge only bumps for levels the user currently cares about
  const isWatched = activeFilters.value.has('all') || activeFilters.value.has(level);
  if (!_isLogTabOpen && isWatched) {
    unreadCount.value++;
  }
}

export const logger = {
  info:    (msg: string) => push('info',    msg),
  warn:    (msg: string) => push('warn',    msg),
  error:   (msg: string) => push('error',   msg),
  success: (msg: string) => push('success', msg),
  clear:   ()            => { logEntries.value = []; unreadCount.value = 0; },
};