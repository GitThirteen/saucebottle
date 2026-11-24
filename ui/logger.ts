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

export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'success';

/** One line of extra info, shown when a log row is expanded. */
export interface LogDetail {
  label: string;
  value: string;
}

export interface LogEntry {
  id: number;
  level: LogLevel;
  message: string;
  /** When the message first came in. */
  timestamp: Date;
  /** When it came in last. Differs from `timestamp` once repeats were merged into this entry. */
  lastSeen: Date;
  /** How often this message was logged. Identical messages close together share one entry. */
  count: number;
  details?: LogDetail[];
}

export interface LogOptions {
  /** Extra info, shown when the row is expanded. */
  details?: LogDetail[];
  /** Set to `false` to keep this message from being merged into an identical earlier one. Default: merge. */
  merge?: boolean;
}

// ── Persistence ──────────────────────────────────────────────────────────────
const STORAGE_KEY = 'saucebottle_log_levels';
const ALL_LEVELS: LogLevel[] = ['debug', 'info', 'success', 'warn', 'error'];

/** Everything except the chatty debug rows. */
const DEFAULT_LEVELS: LogLevel[] = ['info', 'success', 'warn', 'error'];

function loadFilters(): Set<LogLevel> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed: unknown = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return new Set(parsed.filter((l): l is LogLevel => ALL_LEVELS.includes(l as LogLevel)));
      }
    }
  } catch { }
  return new Set(DEFAULT_LEVELS);
}

function saveFilters(filters: Set<LogLevel>) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...filters]));
  } catch { }
}

// ── Core state ───────────────────────────────────────────────────────────────
let _nextId = 0;
const MAX_ENTRIES = 500;

/** Identical messages within this many of the latest entries are merged, so A, B, A, B doesn't flood the log. */
const MERGE_WINDOW = 5;

export const logEntries = ref<LogEntry[]>([]);
export const unreadCount = ref(0);
export const activeFilters = ref<Set<LogLevel>>(loadFilters());

watch(
  activeFilters,
  (val) => saveFilters(val),
  { deep: true }
);

// ── Filter helpers (used by Console.vue and the push function) ───────────────

export function toggleFilter(level: LogLevel) {
  const next = new Set(activeFilters.value);

  if (next.has(level)) next.delete(level);
  else next.add(level);

  activeFilters.value = next;
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

/** Raises the unread badge, but only for levels that are switched on. */
function notifyUnread(level: LogLevel) {
  if (!_isLogTabOpen && activeFilters.value.has(level)) {
    unreadCount.value++;
  }
}

/**
 * Adds a message to the log, or counts it as a repeat of an identical recent one.
 *
 * @returns The id of the entry, usable with `logger.update`.
 */
function push(level: LogLevel, message: string, options: LogOptions = {}): number {
  const now = new Date();

  if (options.merge !== false) {
    const entries = logEntries.value;
    for (let i = entries.length - 1; i >= 0 && i >= entries.length - MERGE_WINDOW; i--) {
      const entry = entries[i];
      if (entry.level === level && entry.message === message) {
        entry.count++;
        entry.lastSeen = now;
        return entry.id;
      }
    }
  }

  const id = _nextId++;
  logEntries.value.push({ id, level, message, timestamp: now, lastSeen: now, count: 1, details: options.details });
  if (logEntries.value.length > MAX_ENTRIES) {
    logEntries.value.splice(0, logEntries.value.length - MAX_ENTRIES);
  }

  notifyUnread(level);
  return id;
}

/**
 * Changes an entry that is already in the log (e.g. a progress line). Does nothing if the entry
 * is gone (cleared or pushed out by newer ones).
 *
 * @returns `true` if the entry was found.
 */
function update(id: number, patch: { level?: LogLevel; message?: string; details?: LogDetail[] }): boolean {
  const entries = logEntries.value;
  for (let i = entries.length - 1; i >= 0; i--) {
    if (entries[i].id !== id) continue;

    const entry = entries[i];
    if (patch.message !== undefined) entry.message = patch.message;
    if (patch.details !== undefined) entry.details = patch.details;
    if (patch.level !== undefined && patch.level !== entry.level) {
      entry.level = patch.level;
      notifyUnread(patch.level);
    }
    return true;
  }
  return false;
}

export const logger = {
  debug:   (msg: string, options?: LogOptions) => push('debug',   msg, options),
  info:    (msg: string, options?: LogOptions) => push('info',    msg, options),
  warn:    (msg: string, options?: LogOptions) => push('warn',    msg, options),
  error:   (msg: string, options?: LogOptions) => push('error',   msg, options),
  success: (msg: string, options?: LogOptions) => push('success', msg, options),
  update,
  clear:   ()            => { logEntries.value = []; unreadCount.value = 0; },
};