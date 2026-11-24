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
import { ref, watch, onActivated, onUnmounted, computed } from 'vue';
import { DropletIcon, AlertTriangleIcon, DownloadCloudIcon, ClockIcon, RotateCcwIcon, BarChart3Icon, ChevronLeftIcon, ChevronRightIcon } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../logger';
import { join, pictureDir, dirname } from '@tauri-apps/api/path';
import { 
  appState, 
  queueCount, 
  activeImage, 
  resultData, 
  errorMessage, 
  loadingMessage, 
  isRareMessage, 
  offlineQueueCount, 
  isPermanentScan,
  updateStatus,
  updateProgress,
  settingsSavedTick
} from '../store';

// ---------------------------------*
// ---- LOCAL STATE (UPDATER) ------*
// ---------------------------------*

const tips = [
  "Tip: Pin your '/input' folder to your OS Quick Access for easier usage!",
  "Tip: Add your API keys in the Credentials tab for a significantly better matching rate.",
  "Tip: You can adjust the invalid match threshold in the Settings tab.",
  "Tip: SauceBottle ignores files that aren't valid images, so don't worry about text files.",
  "Tip: Turn on 'Apply modifications to saved file' to save disk space on massive images.",
  "Tip: You can drag and drop entire folders into the app to scan everything inside.",
  "Tip: Click the Terminal icon at the bottom to check the live logs if a process seems stuck.",
  "Tip: Can't find your sorted images? Check the 'SauceBottle' folder inside your OS Pictures directory.",
];

const kaomojis = [
  "ε=ε=ε=ε=┌(;￣▽￣)┘",
  "ε=ε=ε=┏(゜ロ゜;)┛",
  "ヽ(￣д￣;)ノ=3=3=3",
  "(;° ロ°) 💨",
  "C= C= C=┌( `ー´)┘"
];

const isImageMovedTooFast = ref(false);
const currentKaomoji = ref('');

const showCopied = ref(false);
let copyTimeout: number;

const currentTip = ref(tips[0]);
let tipInterval: number;

const isSlowNetwork = ref(false);
let slowNetworkTimer: number;

// ---------------------------------*
// ---- COMPUTED & WATCHERS --------*
// ---------------------------------*

const badgeClass = computed(() => {
  if (resultData.value.conf < 65) return 'badge-red';
  if (resultData.value.conf < 85) return 'badge-yellow';
  return 'badge-green';
});

// ---------------------------------*
// ---- STATISTICS -----------------*
// ---------------------------------*

interface TopEntry { name: string; detail: string | null; count: number }
interface StatsView {
  enabled: boolean;
  sorted: number;
  originals: number;
  failed: number;
  top_character: TopEntry | null;
  top_fandom: TopEntry | null;
  top_artist: TopEntry | null;
  top_service: TopEntry | null;
  average_confidence: number | null;
  size_kb: number;
  average_sort_millis: number | null;
  saved_millis: number;
  downloads: number;
}

const stats = ref<StatsView | null>(null);

const refreshStats = async () => {
  try {
    stats.value = await invoke<StatsView>('get_stats');
  } catch (e) {
    console.warn('Could not load statistics', e);
  }
};

const formatSize = (kb: number) => {
  if (kb < 1024) return `${kb} KB`;
  if (kb < 1024 * 1024) return `${(kb / 1024).toFixed(1)} MB`;
  return `${(kb / 1024 / 1024).toFixed(2)} GB`;
};

const formatDuration = (totalSeconds: number) => {
  const seconds = Math.round(totalSeconds);
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ${minutes % 60} min`;
  return `${Math.floor(hours / 24)} d ${hours % 24} h`;
};

/** Tile for the most frequent name of one kind (character, fandom, ...). */
const topTile = (label: string, entry: TopEntry | null) => ({
  label,
  value: entry ? entry.name : '–',
  // The fandom goes under the character's name, so same-named characters can be told apart
  sub: entry ? (entry.detail ? `×${entry.count} · ${entry.detail}` : `×${entry.count}`) : '',
  title: entry ? `${entry.name}${entry.detail ? ` (${entry.detail})` : ''}: ${entry.count}×` : '',
});

/** Tiles per page. This is one row of the grid. The order below is the page order. */
const STATS_TILES_PER_PAGE = 3;

const tiles = computed(() => {
  const s = stats.value;
  if (!s) return [];

  const none = '–';
  const avgTime = s.average_sort_millis === null ? none
    : s.average_sort_millis < 1000 ? `${s.average_sort_millis} ms`
    : `${(s.average_sort_millis / 1000).toFixed(1)} s`;
  const attempts = s.sorted + s.failed;

  return [
    // Page 1
    { label: 'Images sorted', value: s.sorted.toLocaleString(), sub: '', title: '' },
    { label: 'Confidence', value: s.average_confidence === null ? none : `${s.average_confidence.toFixed(1)}%`, sub: '', title: 'Average match confidence of the sorted images' },
    {
      label: 'Success rate',
      value: attempts === 0 ? none : `${Math.round((s.sorted / attempts) * 100)}%`,
      sub: attempts === 0 ? '' : `${s.failed} failed`,
      title: 'Images that were sorted, out of all images that were processed',
    },
    // Page 2
    topTile('Top character', s.top_character),
    topTile('Top fandom', s.top_fandom),
    topTile('Top artist', s.top_artist),
    // Page 3
    { label: 'Storage', value: formatSize(s.size_kb), sub: '', title: 'Total size of the sorted images' },
    { label: 'Time per image', value: avgTime, sub: '', title: 'Average time from the start of processing to the finished sort' },
    { label: 'Originals', value: s.originals.toLocaleString(), sub: '', title: 'Sorted images that are original works' },
    // Page 4
    topTile('Top service', s.top_service),
    { label: 'Downloads', value: s.downloads.toLocaleString(), sub: '', title: '' },
    { label: 'Time saved', value: formatDuration(s.saved_millis / 1000), sub: '', title: 'Per sorted image: 30 s of manual sorting minus the time SauceBottle needed (never less than zero)' },
  ];
});

const statsPage = ref(0);

const statsPageCount = computed(() => Math.max(1, Math.ceil(tiles.value.length / STATS_TILES_PER_PAGE)));
const visibleTiles = computed(() => {
  const page = Math.min(statsPage.value, statsPageCount.value - 1);
  return tiles.value.slice(page * STATS_TILES_PER_PAGE, (page + 1) * STATS_TILES_PER_PAGE);
});

const resetStats = async () => {
  const message = 'This permanently deletes all your statistics and starts counting from zero. Your sorted images are not touched.\n\nReset statistics?';
  let confirmed = false;
  try {
    confirmed = await ask(message, { title: 'Reset Statistics', kind: 'warning' });
  } catch {
    confirmed = window.confirm(message);
  }
  if (!confirmed) return;

  try {
    await invoke('reset_stats');
  } catch (e) {
    logger.error(`Could not reset statistics: ${e}`);
  }
  await refreshStats();
};

// This view stays alive when you switch tabs, so refresh on every visit (this also covers the first one) ...
onActivated(refreshStats);
// ... and after settings were saved, e.g. "Collect statistics" switched on or off
watch(settingsSavedTick, refreshStats);

watch(appState, (newState) => {
  clearInterval(tipInterval);

  // Pick up what was sorted in the meantime
  if (newState === 'welcome') refreshStats();

  if (newState === 'updating') {
    tipInterval = window.setInterval(() => {
      let currentIndex = tips.indexOf(currentTip.value);
      let newIndex = Math.floor(Math.random() * (tips.length - 1));
      currentTip.value = tips.filter((_, i) => { return currentIndex !== i; })[newIndex];
    }, 6000);
  }

  clearTimeout(slowNetworkTimer);
  isSlowNetwork.value = false;

  if (newState === 'processing') {
    slowNetworkTimer = window.setTimeout(() => {
      isSlowNetwork.value = true;
    }, 15000);
  }
});

watch(activeImage, () => {
  isImageMovedTooFast.value = false;
});

// ---------------------------------*
// ---- LIFECYCLE ------------------*
// ---------------------------------*

onUnmounted(() => {
  clearInterval(tipInterval);
  clearTimeout(slowNetworkTimer);
  clearTimeout(copyTimeout);
});

// ---------------------------------*
// ---- METHODS --------------------*
// ---------------------------------*

const getAbsoluteDestinationPath = async () => {
  const config: any = await invoke('get_config');
  let basePath = config.output_folder;

  if (!basePath || basePath.trim() === '') {
    const picDir = await pictureDir();
    basePath = await join(picDir, 'SauceBottle', 'results');
  }

  return await join(basePath, resultData.value.dest);
};

const copyPathToClipboard = async () => {
  try {
    const absolutePath = await getAbsoluteDestinationPath();
    await navigator.clipboard.writeText(absolutePath);
    
    showCopied.value = true;
    clearTimeout(copyTimeout);
    copyTimeout = window.setTimeout(() => {
      showCopied.value = false;
    }, 2000);
  } catch (err) {
    console.error("Failed to copy absolute path:", err);
  }
};

const openDestinationFolder = async () => {
  try {
    const absolutePath = await getAbsoluteDestinationPath();
    const folderPath = await dirname(absolutePath);

    await invoke('open_system_folder', { folderTarget: folderPath });
  } catch (err) {
    console.error("Failed to open destination folder:", err);
  }
};

/** Hides the "servers are busy" banner and moves the card back up. It can show again for the next image. */
const dismissSlowNetworkBanner = () => {
  isSlowNetwork.value = false;
};

const handleImageError = () => {
  currentKaomoji.value = kaomojis[Math.floor(Math.random() * kaomojis.length)];
  isImageMovedTooFast.value = true;
};
</script>

<template>
  <div class="main-view">
    <!-- Sits above the card. Its height is animated, so the card slides down to make room and back up when dismissed -->
    <transition name="banner-slide">
      <div v-if="isSlowNetwork" class="banner-slot">
        <div class="banner-slot-inner">
          <div
            class="slow-network-banner unselectable"
            role="button"
            tabindex="0"
            title="Click to dismiss"
            @click="dismissSlowNetworkBanner"
            @keydown.enter="dismissSlowNetworkBanner"
          >
            <ClockIcon :size="24" class="banner-icon" />
            <div class="banner-text">
              <h3>Taking Longer Than Usual</h3>
              <p>The IQDB servers are handling a lot of orders right now. Please be patient, your sauce will be out shortly.</p>
            </div>
          </div>
        </div>
      </div>
    </transition>

    <transition name="fade" mode="out-in">
      <div v-if="appState === 'welcome'" class="welcome-card unselectable">
        <div class="icon-ring organic-blob">
          <DropletIcon :size="32" class="organic-icon" fill="currentColor" />
        </div>
        <h2>What's the Sauce?</h2>
        <p>
          Sort your images by placing them into your <code>/input</code> folder or dropping them here.
          <span class="spacer"></span>
          SauceBottle will identify and categorize them {{ isPermanentScan ? "automatically" : "when you press Run" }}.
        </p>

        <div v-if="offlineQueueCount > 0" class="paused-queue-badge">
          <span class="pulse paused-pulse"></span>
          {{ offlineQueueCount }} {{ offlineQueueCount === 1 ? "image" : "images" }} ready
        </div>
      </div>

      <div v-else-if="appState === 'updating'" class="update-panel processing-card">
        <div class="update-content">
          <DownloadCloudIcon :size="56" class="update-icon pulse-anim" />
          
          <h3 class="update-title">Updating SauceBottle</h3>
          <p class="update-status">{{ updateStatus }}</p>
          
          <div class="progress-container" v-if="updateStatus !== 'Checking for updates...'">
            <div class="progress-track-large">
              <div class="progress-bar-fill" :style="{ width: updateProgress + '%' }"></div>
            </div>
            <span class="progress-text">{{ updateProgress }}%</span>
          </div>

          <div class="tips-box unselectable">
            <transition name="fade" mode="out-in">
              <p :key="currentTip" class="tip-text">{{ currentTip }}</p>
            </transition>
          </div>
        </div>
      </div>

      <div v-else class="processing-card">
        
        <div class="queue-status unselectable">
          <span class="pulse"></span> {{ queueCount }} {{ queueCount === 1 ? "image" : "images" }} in queue
        </div>
        
        <div class="image-wrapper unselectable">
          <div v-if="isImageMovedTooFast" class="overdrive-container">
            <div class="kaomoji-text run-anim">{{ currentKaomoji }}</div>
            <h3 class="overdrive-title">IQDB is in Overdrive!</h3>
            <p class="overdrive-subtext">The server returned the result faster than we could display the image.</p>
          </div>

          <img v-else :key="activeImage" :src="activeImage" alt="Current Image" @error="handleImageError" />
        </div>

        <transition name="panel-slide" mode="out-in">
          
          <div v-if="appState === 'processing'" class="loader-panel">
            <h3 class="loader-text" :class="{ 'rainbow-text': isRareMessage }">
              <template v-if="isRareMessage">
                <span 
                  v-for="(char, idx) in loadingMessage" 
                  :key="idx" 
                  class="wave-char" 
                  :style="{ animationDelay: `${idx * 0.05}s` }"
                >{{ char }}</span>
              </template>
              <template v-else>{{ loadingMessage }}</template>
            </h3>

            <div class="progress-track">
              <div class="progress-bar"></div>
            </div>
          </div>

          <div v-else-if="appState === 'result'" class="info-panel">
            <p class="identified-text" v-if="resultData.name === 'Original' || resultData.isOriginalWork">
              Identified as <span class="highlight">{{ resultData.name === 'Original' ? 'Original Artwork' : resultData.name }}</span><br>
              drawn by <span class="fandom">{{ resultData.artist !== 'Unknown' ? resultData.artist : 'Anon' }}</span>
            </p>
            <p class="identified-text" v-else>
              Identified as <span class="highlight">{{ resultData.name }}</span><br>
              from <span class="fandom">{{ resultData.fandom }}</span>
            </p>
            
            <div class="badge-row">
              <span class="unselectable confidence-badge" :class="badgeClass">{{ resultData.conf }}% Match</span>
              <span class="unselectable service-source">{{ resultData.service }}</span>
            </div>
            
            <div 
              class="destination-box copyable" 
              :title="resultData.dest + '\nLeft-click to open folder\nRight-click to copy path'"
              @click="openDestinationFolder"
              @contextmenu.prevent="copyPathToClipboard"
            >
              <span class="icon">📁</span>
              <code class="path">{{ resultData.dest }}</code>
              
              <transition name="fade">
                <span v-if="showCopied" class="copied-badge">Copied!</span>
              </transition>
            </div>
          </div>

          <div v-else-if="appState === 'error'" class="error-panel">
            <AlertTriangleIcon :size="42" class="error-icon unselectable" />
            <h3 class="error-title">Processing Failed</h3>
            <p class="error-message">{{ errorMessage }}</p>
          </div>

        </transition>
      </div>
    </transition>

    <!-- Optional statistics, welcome screen only. Shows a note while they're off -->
    <transition name="stats-fade">
      <div v-if="appState === 'welcome' && stats" class="stats-card unselectable">
        <div v-if="!stats.enabled" class="stats-off">
          <BarChart3Icon :size="26" class="stats-off-icon" />
          <span class="stats-off-title">Statistics are off</span>
          <span class="stats-off-text">Tick <b>Collect statistics</b> in the settings to see your sorting numbers here.</span>
        </div>

        <template v-else>
          <div class="stats-header">
            <h3>Statistics</h3>
            <div class="stats-controls">
              <template v-if="statsPageCount > 1">
                <button class="stats-icon-btn" :disabled="statsPage === 0" title="Previous" aria-label="Previous page" @click="statsPage--">
                  <ChevronLeftIcon :size="14" />
                </button>
                <span class="stats-page">{{ statsPage + 1 }} / {{ statsPageCount }}</span>
                <button class="stats-icon-btn" :disabled="statsPage >= statsPageCount - 1" title="Next" aria-label="Next page" @click="statsPage++">
                  <ChevronRightIcon :size="14" />
                </button>
              </template>
              <button class="stats-icon-btn" title="Reset statistics" aria-label="Reset statistics" @click="resetStats">
                <RotateCcwIcon :size="14" />
              </button>
            </div>
          </div>

          <div class="stats-grid">
            <div v-for="tile in visibleTiles" :key="tile.label" class="stats-tile" :title="tile.title">
              <span class="stats-value">{{ tile.value }}</span>
              <span v-if="tile.sub" class="stats-sub">{{ tile.sub }}</span>
              <span class="stats-label">{{ tile.label }}</span>
            </div>
          </div>
        </template>
      </div>
    </transition>

  </div>
</template>

<style scoped>
.main-view { 
  padding: 20px; 
  display: flex; 
  flex-direction: column; 
  gap: 20px; 
  box-sizing: border-box; 
  height: 100%;
  justify-content: flex-start;
}

/* Cards and the banner must never shrink to fit the tab, the tab scrolls instead. Without this,
   the card (overflow: hidden) shrinks below its content as soon as the banner takes space. */
.main-view > * {
  flex-shrink: 0;
}

.spacer {
  display: block;
  height: 4px;
}

/* -- UPDATER SCREEN -- */
.update-panel {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  min-height: 400px;
}

.update-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 80%;
}

.update-icon {
  color: var(--accent-primary);
  margin-bottom: 20px;
}

.update-title {
  margin: 0 0 5px 0;
  font-size: 1.4rem;
  color: var(--text-primary);
  font-weight: 800;
}

.update-status {
  margin: 0 0 35px 0;
  font-size: 0.9rem;
  color: var(--text-secondary);
  font-family: monospace;
}

.progress-container {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 45px;
}

.progress-track-large {
  width: 100%;
  height: 10px;
  background: var(--bg-base);
  border-radius: 6px;
  overflow: hidden;
  border: 1px solid var(--bg-surface-elevated);
}

.progress-bar-fill {
  height: 100%;
  background: var(--accent-gradient);
  border-radius: 6px;
  transition: width 0.3s ease;
}

.progress-text {
  font-size: 0.85rem;
  font-weight: 800;
  color: var(--text-tertiary);
  align-self: flex-end;
}

.tips-box {
  background: rgba(0, 0, 0, 0.15);
  border: 1px dashed var(--bg-surface-elevated);
  padding: 15px 25px;
  border-radius: 8px;
  width: 100%;
  min-height: 52px; 
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
}

.tip-text {
  margin: 0;
  font-size: 0.85rem;
  color: var(--text-secondary);
  line-height: 1.4;
  font-style: italic;
}

/* -- WELCOME SCREEN -- */
.welcome-card { 
  background: var(--bg-surface); 
  border-radius: 12px;
  padding: 40px 20px;
  text-align: center;
  border: 1px solid var(--bg-surface-elevated);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}

.icon-ring {
  width: 70px; 
  height: 70px; 
  border-radius: 50%; 
  background: rgba(255, 75, 43, 0.1); 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  margin-bottom: 20px; 
  border: 1px solid rgba(255, 75, 43, 0.2); 
}

.sparkle-icon { 
  color: var(--accent-primary); 
  animation: float 3s ease-in-out infinite; 
}

.welcome-card h2 { 
  margin: 0 0 10px 0; 
  color: var(--text-primary); 
  font-size: 1.5rem; 
}

.welcome-card p { 
  color: var(--text-secondary); 
  font-size: 0.95rem; 
  line-height: 1.5; 
  margin: 0; 
}

.welcome-card code { 
  background: var(--bg-base); 
  padding: 2px 6px; 
  border-radius: 4px; 
  color: var(--text-primary); 
  font-family: monospace; 
  font-size: 0.85rem; 
}

/* -- STATISTICS CARD (welcome screen only) -- */
.stats-card {
  background: var(--bg-surface);
  border-radius: 12px;
  padding: 14px;
  border: 1px solid var(--bg-surface-elevated);
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.stats-header { display: flex; align-items: center; justify-content: space-between; }
.stats-header h3 { margin: 0; font-size: 1.05rem; color: var(--text-primary); }

.stats-controls { display: flex; align-items: center; gap: 6px; }
.stats-page { min-width: 34px; text-align: center; color: var(--text-secondary); font-size: 0.72rem; font-weight: 700; }

.stats-icon-btn {
  display: flex; align-items: center; justify-content: center;
  width: 26px; height: 26px; padding: 0;
  background: transparent; color: var(--text-tertiary);
  border: 1px solid var(--bg-surface-elevated); border-radius: 6px;
  cursor: pointer; transition: color 0.2s, border-color 0.2s;
}
.stats-icon-btn:hover:not(:disabled) { color: var(--text-primary); border-color: var(--accent-primary); }

.stats-off { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 10px 0 4px; text-align: center; }
.stats-off-icon { color: var(--text-tertiary); margin-bottom: 4px; }
.stats-off-title { color: var(--text-primary); font-weight: 800; font-size: 0.95rem; }
.stats-off-text { color: var(--text-secondary); font-size: 0.85rem; line-height: 1.5; max-width: 280px; }
.stats-off-text b { color: var(--text-primary); }

.stats-icon-btn:disabled { opacity: 0.35; cursor: default; }

/* All tiles have the same size: equal columns, fixed row height */
.stats-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); grid-auto-rows: 60px; gap: 8px; }
.stats-tile {
  display: flex; flex-direction: column; gap: 1px; min-width: 0; min-height: 0; box-sizing: border-box;
  padding: 8px 10px; background: var(--bg-base); border-radius: 8px;
}
.stats-value { color: var(--text-primary); font-weight: 800; font-size: 0.85rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stats-sub { color: var(--text-secondary); font-size: 0.62rem; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.stats-label { margin-top: auto; padding-top: 2px; color: var(--text-tertiary); font-size: 0.58rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.03em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* Fades in a bit after the welcome card, so it doesn't pop up while the previous card is still leaving */
.stats-fade-enter-active { transition: opacity 0.3s ease 0.3s; }
.stats-fade-leave-active { transition: opacity 0.3s ease; }
.stats-fade-enter-from, .stats-fade-leave-to { opacity: 0; }

/* -- PROCESSING CARD (Shared wrapper) -- */
.processing-card { 
  background: var(--bg-surface); 
  border-radius: 12px; 
  box-shadow: 0 8px 30px rgba(0,0,0,0.5);
  border: 1px solid var(--bg-surface-elevated);
  /* Clips the children to the rounded corners. The queue bar's square background covered the border there */
  overflow: hidden;
}

/* Top Status Bar & Image */
.queue-status { 
  padding: 10px; 
  font-size: 0.8rem; 
  color: var(--text-secondary); 
  background: var(--bg-base); 
  border-bottom: 1px solid var(--bg-surface-elevated); 
  display: flex; 
  align-items: center; 
  gap: 8px; 
  justify-content: center; 
  font-weight: 700; 
}

.pulse { 
  width: 8px; 
  height: 8px; 
  background: var(--accent-primary); 
  border-radius: 50%; 
  box-shadow: 0 0 8px var(--accent-primary); 
  animation: pulse-anim 2s infinite; 
}

.image-wrapper { 
  height: 260px; 
  width: 100%; 
  overflow: hidden; 
  background: #000; 
  display: flex; 
  justify-content: center; 
  align-items: center; 
  border-bottom: 1px solid var(--bg-surface-elevated); 
}

.image-wrapper img { 
  width: 100%; 
  height: 100%; 
  object-fit: cover; 
  animation: subtle-zoom 10s infinite alternate linear; 
}

/* -- STATE 1: LOADER ANIMATION -- */
.loader-panel { 
  padding: 40px 30px; 
  text-align: center; 
}

.loader-text { 
  margin: 0 0 20px 0; 
  font-size: 1.1rem; 
  color: var(--accent-primary); 
  font-weight: 800; 
  letter-spacing: 1px; 
  animation: pulse-text 1.5s infinite; 
}

.progress-track { 
  height: 6px; 
  background: var(--bg-base); 
  border-radius: 4px; 
  overflow: hidden; 
  position: relative; 
}

.progress-bar { 
  position: absolute; 
  top: 0; 
  left: 0; 
  width: 40%; 
  height: 100%; 
  background: var(--accent-gradient); 
  border-radius: 4px; 
  animation: scan 1.2s infinite ease-in-out alternate; 
}

/* -- STATE 2: INFO PANEL -- */
.info-panel { 
  padding: 20px; 
  text-align: center; 
}

.identified-text { 
  margin: 0 0 10px 0; 
  font-size: 1.2rem; 
  color: var(--text-secondary); 
}
.identified-text .highlight { 
  color: var(--text-primary); 
  font-size: 1.4rem; 
  font-weight: 800; 
}
.identified-text .fandom { 
  font-size: 1rem; 
  font-weight: 700; 
}

.badge-row { 
  display: flex; 
  justify-content: center; 
  gap: 10px; 
  margin-bottom: 20px; 
}
.confidence-badge { 
  padding: 4px 12px; 
  border-radius: 20px; 
  font-size: 0.75rem; 
  font-weight: 800; 
}
.badge-green { 
  background: rgba(0, 230, 118, 0.15); 
  color: #00e676; 
  border: 1px solid rgba(0, 230, 118, 0.3); 
}
.badge-yellow { 
  background: rgba(245, 197, 66, 0.15); 
  color: #f5c542; 
  border: 1px solid rgba(245, 197, 66, 0.3); 
}
.badge-red { 
  background: rgba(255, 77, 77, 0.15); 
  color: #ff4d4d; 
  border: 1px solid rgba(255, 77, 77, 0.3); 
}
.service-source { 
  background: var(--bg-base); 
  color: var(--text-secondary); 
  padding: 4px 12px; 
  border-radius: 20px; 
  font-size: 0.75rem; 
  font-weight: 800; 
  border: 1px solid var(--bg-surface-elevated); 
}

.destination-box { 
  display: flex; 
  align-items: center; 
  gap: 10px; 
  background: var(--bg-base); 
  padding: 10px 15px; 
  border-radius: 8px; 
  border: 1px dashed var(--text-tertiary); 
  position: relative;
}
.destination-box.copyable {
  cursor: pointer;
  transition: border-color 0.2s ease, background 0.2s ease;
}
.destination-box.copyable:hover {
  border-color: var(--accent-primary);
  background: rgba(255, 75, 43, 0.05);
}
.destination-box .path {
  font-family: monospace;
  color: var(--accent-primary);
  font-size: 0.85rem;
  font-weight: 700;

  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.copied-badge {
  position: absolute;
  right: 15px;
  background: rgba(0, 230, 118, 0.25); 
  color: #00e676;
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 800;
  border: 1px solid rgba(0, 230, 118, 0.4);
  pointer-events: none; 
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}

/* -- STATE 3: ERROR PANEL -- */
.error-panel { 
  padding: 30px; 
  text-align: center; 
  display: flex; 
  flex-direction: column; 
  align-items: center; 
}
.error-icon { 
  color: #ff4d4d; 
  margin-bottom: 12px; 
  animation: pulse-text 1.5s infinite; 
}
.error-title { 
  margin: 0 0 10px 0; 
  font-size: 1.2rem; 
  color: #ff4d4d; 
  font-weight: 800; 
}
.error-message { 
  margin: 0; 
  font-size: 0.85rem; 
  color: var(--text-secondary); 
  background: rgba(255, 77, 77, 0.1); 
  padding: 12px 16px; 
  border-radius: 8px; 
  border: 1px dashed rgba(255, 77, 77, 0.3); 
  word-break: break-word; 
  line-height: 1.4; 
  font-family: monospace; 
  max-width: 90%; 
}

/* -- SLOW NETWORK BANNER -- */
/* Sits above the card. The slot's height is animated (0fr -> 1fr), so the card below slides down
   to make room and back up when the banner goes away. */
.banner-slot {
  display: grid;
  grid-template-rows: 1fr;
  /* Cancels the flex gap of the container. The spacing below the banner is part of the slot's own
     (animated) height instead, so nothing jumps when the slot appears or disappears. */
  margin-bottom: -20px;
  /* The container's top padding is 20px, but with the banner shown only 12px should be above it
     (the same as below it). Animated together with the height, see the transition below. */
  margin-top: -8px;
}

.banner-slot-inner {
  min-height: 0;
  overflow: hidden;
}

/* Same layout as the info banner in the Credentials tab, in the warning colors */
.slow-network-banner {
  box-sizing: border-box;
  margin-bottom: 12px; /* the space between banner and card */

  display: flex;
  align-items: center;
  gap: 15px;
  background: rgba(245, 197, 66, 0.1);
  border: 1px solid rgba(245, 197, 66, 0.3);
  padding: 15px;
  border-radius: 8px;

  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease;
}

.slow-network-banner .banner-text {
  flex: 1;
}

.slow-network-banner .banner-text h3 {
  margin: 0 0 5px 0;
  font-size: 0.95rem;
  color: #f5c542;
}

.slow-network-banner .banner-text p {
  margin: 0;
  font-size: 0.8rem;
  color: var(--text-secondary);
  line-height: 1.4;
}

.slow-network-banner:hover,
.slow-network-banner:focus-visible {
  background: rgba(245, 197, 66, 0.16);
  border-color: rgba(245, 197, 66, 0.5);
  outline: none;
}

.banner-icon {
  color: #f5c542;
  flex-shrink: 0;
}

.banner-slide-enter-active,
.banner-slide-leave-active {
  transition: grid-template-rows 0.5s cubic-bezier(0.22, 1, 0.36, 1), margin-top 0.5s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.35s ease;
}

.banner-slide-enter-from,
.banner-slide-leave-to {
  grid-template-rows: 0fr;
  margin-top: 0;
  opacity: 0;
}

/* The banner itself slides in from above while the space opens up */
.banner-slide-enter-active .slow-network-banner,
.banner-slide-leave-active .slow-network-banner {
  transition: transform 0.5s cubic-bezier(0.22, 1, 0.36, 1), background 0.2s ease, border-color 0.2s ease;
}

.banner-slide-enter-from .slow-network-banner,
.banner-slide-leave-to .slow-network-banner {
  transform: translateY(-16px);
}

/* -- RARE EASTER EGG TEXT -- */
.rainbow-text {
  display: inline-block;
  font-size: 1.1rem;
  text-transform: uppercase;
  white-space: pre-wrap;
}

.wave-char {
  display: inline-block;
  color: #ff4d4d;
  animation: mexican-wave 1.2s ease-in-out infinite, color-shift 2.5s linear infinite;
}

@keyframes color-shift {
  0% { filter: hue-rotate(0deg) saturate(1.5); }
  100% { filter: hue-rotate(360deg) saturate(1.5); }
}

@keyframes mexican-wave {
  0%, 40%, 100% { transform: translateY(0); }
  20% { transform: translateY(-8px); }
}

/* -- PAUSED BADGE -- */
.paused-queue-badge {
  margin-top: 25px;
  background: rgba(245, 197, 66, 0.15);
  color: #f5c542;
  padding: 10px 20px;
  border-radius: 999px;
  font-size: 0.85rem;
  font-weight: 800;
  display: flex;
  align-items: center;
  gap: 10px;
  border: 1px solid rgba(245, 197, 66, 0.3);
  animation: pop 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}

.paused-pulse {
  background: #f5c542;
  box-shadow: 0 0 8px #f5c542;
}

/* -- BLOB ANIMATION -- */
.organic-blob {
  animation: blob-morph 5s ease-in-out infinite alternate;
  background: rgba(255, 75, 43, 0.1);
  border: 1px solid rgba(255, 75, 43, 0.2);
}

.organic-icon {
  color: var(--accent-primary);
  animation: liquid-sway 4s ease-in-out infinite;
}

@keyframes blob-morph {
   0% { border-radius: 50%; }
   50% { border-radius: 60% 40% 40% 60% / 50% 60% 40% 50%; }
   100% { border-radius: 40% 60% 60% 40% / 60% 40% 50% 50%; }
}

@keyframes liquid-sway {
  0%, 100% { transform: translateY(0px) scale(1) rotate(0deg); }
  33% { transform: translateY(-4px) scale(1.05) rotate(4deg); }
  66% { transform: translateY(-2px) scale(0.95) rotate(-4deg); }
}

/* -- OVERDRIVE MODE -- */
.overdrive-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 20px;
  width: 100%;
  height: 100%;
  background: repeating-linear-gradient(
    45deg,
    rgba(0, 0, 0, 0.2),
    rgba(0, 0, 0, 0.2) 10px,
    rgba(255, 75, 43, 0.05) 10px,
    rgba(255, 75, 43, 0.05) 20px
  );
}

.kaomoji-text {
  font-size: 2rem;
  color: var(--accent-primary);
  margin-bottom: 18px;
  font-family: monospace;
  white-space: nowrap;
}

.overdrive-title {
  margin: 0 0 5px 0;
  font-size: 1.2rem;
  color: var(--text-primary);
  font-weight: 800;
}

.overdrive-subtext {
  margin: 0;
  font-size: 0.85rem;
  color: var(--text-secondary);
  max-width: 80%;
  line-height: 1.4;
}

.run-anim {
  animation: run-bob 0.4s infinite alternate linear;
}

@keyframes run-bob {
  0% { transform: translateY(0) translateX(-5px) rotate(-2deg); }
  100% { transform: translateY(-5px) translateX(5px) rotate(2deg); }
}


/* -- ANIMATIONS & TRANSITIONS -- */
@keyframes float { 
  0% { transform: translateY(0px); } 
  50% { transform: translateY(-8px); } 
  100% { transform: translateY(0px); } 
}
@keyframes pulse-anim { 
  0% { opacity: 1; transform: scale(1); } 
  50% { opacity: 0.5; transform: scale(1.2); } 
  100% { opacity: 1; transform: scale(1); } 
}
@keyframes pulse-text { 
  0%, 100% { opacity: 1; } 
  50% { opacity: 0.6; } 
}
@keyframes scan { 
  0% { transform: translateX(-100%); } 
  100% { transform: translateX(250%); } 
}
@keyframes subtle-zoom { 
  0% { transform: scale(1); } 
  100% { transform: scale(1.05); } 
}
@keyframes pop { 
  0% { transform: scale(0.85); opacity: 0; } 
  100% { transform: scale(1); opacity: 1; } 
}

.fade-enter-active, .fade-leave-active { transition: opacity 0.3s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

.panel-slide-enter-active, .panel-slide-leave-active { transition: all 0.3s ease; }
.panel-slide-enter-from { opacity: 0; transform: translateY(10px); }
.panel-slide-leave-to { opacity: 0; transform: translateY(-10px); }
</style>