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
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { 
  HomeIcon, 
  SettingsIcon, 
  TerminalIcon, 
  PlayIcon, 
  PauseIcon, 
  SquareIcon,
  KeyIcon, 
  DownloadIcon, 
  UploadCloudIcon, 
  FolderOpenIcon 
} from 'lucide-vue-next';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ask } from '@tauri-apps/plugin-dialog';

import CredentialsView from './components/Credentials.vue';
import MainView from './components/Main.vue';
import SettingsView from './components/Settings.vue';
import LogsView from './components/Console.vue';
import DownloadView from './components/Download.vue';

import {
  isPermanentScan,
  isProcessing,
  autoUpdateEnabled,
  initTauriListeners,
  hasCredentials,
  refreshVaultStatus,
  syncOfflineQueue,
  appState,
  updateProgress,
  updateStatus,
  queueCount,
  offlineQueueCount
} from './store';
import { unreadCount, setLogTabOpen } from './logger';
import './assets/main.css';

// ---------------------------------*
// ---- LOCAL STATE ----------------*
// ---------------------------------*

const currentTab = ref('main');
const isDragging = ref(false);
let terminateTimeout: number | undefined;
let unlistenDragDrop: (() => void) | undefined;

// ---------------------------------*
// ---- COMPUTED -------------------*
// ---------------------------------*

/** * Controls the pulsing "breathing" animation on the play/pause button.
 * Only breathes if the app is actively listening AND hasn't been paused by the user.
 */
const isBreathing = computed(() => isPermanentScan.value && isProcessing.value);

// ---------------------------------*
// ---- METHODS --------------------*
// ---------------------------------*

/**
 * Specifically switches to the logs view and marks the logger as "open" 
 * so it clears the unread notification badge.
 */
const openLogsTab = () => {
  currentTab.value = 'logs';
  setLogTabOpen(true);
};

/**
 * Handles switching between the main application tabs.
 * Manages the read/unread state of the logger depending on if we are entering or leaving the logs tab.
 * * @param tab - The string identifier of the tab to switch to.
 */
const setTab = (tab: string) => {
  if (currentTab.value === 'logs' && tab !== 'logs') setLogTabOpen(false);
  if (tab === 'logs') setLogTabOpen(true);
  
  currentTab.value = tab;
};

/**
 * Toggles the background scanning daemon on and off.
 */
const toggleProcessing = async () => {
  isProcessing.value = !isProcessing.value;
  try {
    await invoke('set_scan_state', { active: isProcessing.value });
  } catch (e) {
    console.error("Failed to sync state with Rust:", e);
  }
};

/**
 * Asks the Rust backend to spawn the native OS file explorer 
 * (Windows Explorer, macOS Finder, etc.) pointing directly to the sorted results folder.
 */
const openResultsFolder = async () => {
  try { 
    await invoke('open_system_folder', { folderTarget: 'results' }); 
  } catch (e) { 
    console.error("Failed to open results folder", e); 
  }
};

const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

/**
 * Initiates the app boot sequence.
 * Forces the UI into the 'updating' loading screen for a minimum of 7.5 seconds while
 * concurrently checking GitHub for a new release.
 */
const runBootSequence = async () => {
  if (!autoUpdateEnabled.value) {
    console.log("Auto-updates are disabled in settings. Skipping check.");
    
    appState.value = 'welcome';
    return;
  }

  appState.value = 'updating';
  updateStatus.value = 'Checking for updates...';
  updateProgress.value = 0; 

  try {
    const [update, _] = await Promise.all([
      check().catch((e) => {
        console.error("Update check failed (offline or rate limited):", e);
        return null; // Return null gracefully if the user, e.g., has no internet
      }),
      sleep(7500)
    ]);
    
    if (update) {
      const wantsUpdate = await ask(
        `SauceBottle v${update.version} is available!\n\nRelease notes:\n${update.body}\n\nWould you like to install it now?`, 
        { title: 'Update Available', kind: 'info' }
      );

      if (wantsUpdate) {
        let downloaded = 0;
        let contentLength = 0;

        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case 'Started':
              contentLength = event.data.contentLength || 0;
              updateStatus.value = `Downloading v${update.version}...`;
              break;
            case 'Progress':
              downloaded += event.data.chunkLength;
              if (contentLength > 0) {
                updateProgress.value = Math.round((downloaded / contentLength) * 100);
              }
              break;
            case 'Finished':
              updateStatus.value = 'Installing...';
              updateProgress.value = 100;
              break;
          }
        });

        updateStatus.value = 'Restarting SauceBottle...';
        await relaunch(); 
        return;
      }
    }
  } catch (error) {
    console.error("Critical error during boot sequence:", error);
  }

  appState.value = 'welcome';
};

// ---------------------------------*
// ---- LIFECYCLE ------------------*
// ---------------------------------*

onMounted(async () => {
  // 1. Initial State Sync
  await refreshVaultStatus();
  await syncOfflineQueue();

  // 2. Load basic config for UI toggles
  try {
    const config: any = await invoke('get_config');
    if (config.flags) {
        if (typeof config.flags.isPermanentScan === 'boolean') {
          isPermanentScan.value = config.flags.isPermanentScan;
          isProcessing.value = config.flags.isPermanentScan;
        }
        if (typeof config.flags.autoUpdateEnabled === 'boolean') {
          autoUpdateEnabled.value = config.flags.autoUpdateEnabled;
        }
      }
  } catch (e) {
    console.warn("Could not load config on boot");
  }

  // 3. Check for updates
  await runBootSequence();

  // 4. Bind Tauri Events
  await initTauriListeners();

  // 5. Tell Rust the UI is ready so it can process anything left over from last time
  try {
    await invoke('frontend_ready');
  } catch (e) {
    console.error("Failed to trigger initial sweep:", e);
  }

  // 6. Setup native drag-and-drop file interception
  try {
    unlistenDragDrop = await getCurrentWindow().onDragDropEvent(async (event) => {
      if (event.payload.type === 'over' || event.payload.type === 'enter') {
        isDragging.value = true;
      } else if (event.payload.type === 'leave') {
        isDragging.value = false;
      } else if (event.payload.type === 'drop') {
        isDragging.value = false;
        if (event.payload.paths && event.payload.paths.length > 0) {
          try {
            const resultMessage = await invoke('process_dropped_files', { paths: event.payload.paths });
            console.log(resultMessage);
            await syncOfflineQueue();
            if (!isProcessing.value) appState.value = 'welcome';
          } catch (errorMessage) {
            alert(`Drop Aborted:\n${errorMessage}`);
          }
        }
      }
    });
  } catch (e) {
    console.warn("Could not bind drag and drop listener", e);
  }
});

onUnmounted(() => {
  unlistenDragDrop?.();
});

watch(() => appState.value, async (newState) => {
  if (!isPermanentScan.value && isProcessing.value && newState === 'welcome') {
    if (queueCount.value === 0 && offlineQueueCount.value === 0) {
      isProcessing.value = false;
      try {
        await invoke('set_scan_state', { active: false });
      } catch (e) {
        console.error("Failed to terminate scan state:", e);
      }
    }
  }
});

watch([queueCount, isProcessing], ([newQueue, newProcessing]) => {
  if (!isPermanentScan.value && !newProcessing && newQueue === 0) {
    clearTimeout(terminateTimeout);
    terminateTimeout = window.setTimeout(() => {
      // If we are in manual mode, the user clicked Terminate (isProcessing is false), 
      // and the final image just finished processing (queue hit 0), force the welcome screen.
      if (!isProcessing.value && queueCount.value === 0) {
        appState.value = 'welcome';
      }
      
    }, 4000);
  }
});
</script>

<template>
  <div class="app-container">
    <header class="titlebar unselectable">
      <button class="header-folder-btn" @click="openResultsFolder" title="Open Results Folder">
        <FolderOpenIcon :size="18" />
      </button>

      <h1 class="logo">SAUCEBOTTLE</h1>
      
      <button
        class="header-play-btn"
        :class="{ breathing: isBreathing }"
        @click="toggleProcessing"
        :title="isProcessing ? (isPermanentScan ? 'Pause' : 'Terminate') : 'Run'"
      >
        <PauseIcon v-if="isProcessing && isPermanentScan" :size="18" />
        <SquareIcon v-else-if="isProcessing && !isPermanentScan" :size="18" fill="currentColor" />
        <PlayIcon v-else :size="18" fill="currentColor" />
      </button>
    </header>

    <div class="drag-overlay" v-if="isDragging">
      <div class="drag-content">
        <h2>Drop to Sort</h2>
        <UploadCloudIcon :size="64" />
        <p>Image files will be moved. Folder contents will be copied to /input.</p>
      </div>
    </div>

    <main class="content">
      <KeepAlive>
        <MainView v-if="currentTab === 'main'" />
      </KeepAlive>
      <DownloadView v-if="currentTab === 'download'" />
      <LogsView v-if="currentTab === 'logs'" />
      <CredentialsView v-if="currentTab === 'keys'" />
      <SettingsView v-if="currentTab === 'settings'" />
    </main>

    <nav class="bottom-nav unselectable">
      <div class="nav-pill">
        <button :class="{ active: currentTab === 'main' }" @click="setTab('main')" title="Home">
          <HomeIcon :size="22" />
        </button>
        <button :class="{ active: currentTab === 'download' }" @click="setTab('download')" title="Download">
          <DownloadIcon :size="22" />
        </button>
        <button
          :class="{ active: currentTab === 'logs' }"
          @click="openLogsTab"
          title="Logs"
          class="relative-btn"
        >
          <span v-if="unreadCount > 0" class="log-badge">{{ unreadCount > 99 ? '∞' : unreadCount }}</span>
          <TerminalIcon :size="22" />
        </button>
        <button :class="{ active: currentTab === 'keys' }" @click="setTab('keys')" title="Credentials" class="relative-btn">
          <span v-if="!hasCredentials" class="alert-badge">!</span>
          <KeyIcon :size="22" />
        </button>
        <button :class="{ active: currentTab === 'settings' }" @click="setTab('settings')" title="Settings">
          <SettingsIcon :size="22" />
        </button>
      </div>
    </nav>
  </div>
</template>

<style scoped>
.app-container { 
  display: flex; 
  flex-direction: column; 
  height: 100vh; 
  position: relative; 
}

/* -- Header -- */
.titlebar { 
  height: 65px; 
  position: relative; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  background: var(--bg-surface); 
  border-bottom: 1px solid var(--bg-surface-elevated); 
  overflow: hidden; 
  flex-shrink: 0;
}

.logo { 
  font-family: 'Righteous', cursive; 
  font-size: 1.4rem; 
  letter-spacing: 2px; 
  background: var(--accent-gradient); 
  -webkit-background-clip: text; 
  background-clip: text; 
  -webkit-text-fill-color: transparent; 
}

.header-play-btn { 
  position: absolute; 
  right: 20px; 
  background: var(--accent-gradient); 
  color: white; 
  border: none; 
  border-radius: 50%; 
  width: 36px; 
  height: 36px; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  cursor: pointer; 
  box-shadow: 0 4px 12px rgba(255, 75, 43, 0.4); 
  transition: transform 0.2s, box-shadow 0.2s; 
}
.header-play-btn:hover { 
  transform: scale(1.1); 
  box-shadow: 0 6px 16px rgba(255, 75, 43, 0.6); 
}
.header-play-btn:active { 
  transform: scale(0.95); 
}
.header-play-btn.breathing { 
  animation: breathe 2.8s ease-in-out infinite; 
}

.header-folder-btn {
  position: absolute;
  left: 20px;
  background: var(--bg-surface-elevated);
  color: var(--text-secondary);
  border: 1px solid transparent;
  border-radius: 50%;
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}
.header-folder-btn:hover {
  color: var(--accent-primary);
  border-color: rgba(255, 75, 43, 0.3);
  transform: scale(1.1);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
}
.header-folder-btn:active {
  transform: scale(0.95);
}

@keyframes breathe {
  0%   { box-shadow: 0 4px 12px rgba(255, 75, 43, 0.3); transform: scale(1); }
  50%  { box-shadow: 0 4px 24px rgba(255, 75, 43, 0.7); transform: scale(1.1); }
  100% { box-shadow: 0 4px 12px rgba(255, 75, 43, 0.3); transform: scale(1); }
}

/* -- Main Content -- */
.content { 
  flex: 1; 
  overflow: overlay; 
  overflow-x: hidden; 
  position: relative; 
  padding-bottom: 90px; 
}

/* -- Bottom Nav -- */
.bottom-nav { 
  position: absolute;
  bottom: 20px;
  left: 50%;
  transform: translateX(-50%);
  display: flex; 
  justify-content: center; 
  align-items: center; 
  pointer-events: none;
  z-index: 50;
}

.nav-pill { 
  display: flex; 
  align-items: center; 
  background: var(--bg-surface); 
  border-radius: 999px; 
  padding: 6px; 
  gap: 4px; 
  box-shadow: 0 4px 24px rgba(0,0,0,0.4); 
  border: 1px solid var(--bg-surface-elevated); 
  pointer-events: auto; /* Re-enable clicks specifically for the pill itself */
}

.nav-pill button { 
  width: 52px; 
  height: 52px; 
  border-radius: 50%; 
  background: none; 
  border: none; 
  color: var(--text-tertiary); 
  cursor: pointer; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  transition: background 0.2s, color 0.2s, transform 0.15s; 
}
.nav-pill button:hover { 
  color: var(--text-primary); 
  transform: scale(1.08); 
}
.nav-pill button:active { 
  transform: scale(0.93); 
}
.nav-pill button.active { 
  background: var(--accent-gradient); 
  color: #fff; 
  box-shadow: 0 2px 12px rgba(255, 75, 43, 0.4); 
}

/* -- Badges -- */
.relative-btn { position: relative; }
.alert-badge { 
  position: absolute; 
  top: 4px; 
  right: 4px; 
  background: #ff4d4d; 
  color: white; 
  font-size: 0.65rem; 
  font-weight: 900; 
  width: 14px; 
  height: 14px; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  border-radius: 50%; 
  border: 2px solid var(--bg-surface); 
  animation: pulse-alert 2s infinite; 
}
.log-badge { 
  position: absolute; 
  top: 4px; 
  right: 4px; 
  background: #60a5fa; 
  color: #000; 
  font-size: 0.55rem; 
  font-weight: 900; 
  min-width: 14px; 
  height: 14px; 
  padding: 0 3px; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  border-radius: 999px; 
  border: 2px solid var(--bg-surface); 
}

@keyframes pulse-alert { 
  0% { box-shadow: 0 0 0 0 rgba(255, 77, 77, 0.7); } 
  70% { box-shadow: 0 0 0 6px rgba(255, 77, 77, 0); } 
  100% { box-shadow: 0 0 0 0 rgba(255, 77, 77, 0); } 
}

/* -- Drag Overlay -- */
.drag-overlay { 
  position: fixed; 
  top: 0; 
  left: 0; 
  width: 100vw; 
  height: 100vh; 
  background: rgba(0,0,0,0.85); 
  z-index: 9999; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  backdrop-filter: blur(8px); 
}

.drag-content { 
  border: 2px dashed var(--accent-primary); 
  border-radius: 24px; 
  padding: 30px 50px; 
  margin: 15px; 
  display: flex; 
  flex-direction: column; 
  align-items: center; 
  justify-content: center; 
  gap: 15px; 
  color: var(--accent-primary); 
  background: rgba(255, 75, 43, 0.1); 
  animation: pop 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275); 
  pointer-events: none; 
  text-align: center; 
}
.drag-content h2 { margin: 0; font-size: 2rem; color: #fff; }
.drag-content p { margin: 0; color: var(--text-secondary); font-weight: 500; font-size: 0.8rem; }

@keyframes pop { 
  0% { transform: scale(0.85); opacity: 0; } 
  100% { transform: scale(1); opacity: 1; } 
}
</style>