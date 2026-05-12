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
import { ref, watch, onUnmounted, computed } from 'vue';
import { DropletIcon, AlertTriangleIcon, DownloadCloudIcon, ClockIcon } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { join, pictureDir } from '@tauri-apps/api/path';
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
  updateProgress
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

const panickingKaomojis = [
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

watch(() => appState.value, (newState) => {
  if (newState === 'updating') {
    let currentIndex = tips.indexOf(currentTip.value);

    tipInterval = window.setInterval(() => {
      let newIndex;
      
      do {
        newIndex = Math.floor(Math.random() * tips.length);
      } while (newIndex === currentIndex && tips.length > 1);

      currentIndex = newIndex;
      currentTip.value = tips[currentIndex];
    }, 6000);
  } else {
    clearInterval(tipInterval);
  }

  clearTimeout(slowNetworkTimer);
  isSlowNetwork.value = false;

  if (newState === 'processing') {
    slowNetworkTimer = window.setTimeout(() => {
      isSlowNetwork.value = true;
    }, 12000);
  }
});

watch(() => activeImage.value, () => {
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

const copyPathToClipboard = async () => {
  try {
    const config: any = await invoke('get_config');
    let basePath = config.output_folder;

    if (!basePath || basePath.trim() === '') {
      const picDir = await pictureDir();
      basePath = await join(picDir, 'SauceBottle');
    }

    const absolutePath = await join(basePath, resultData.value.dest);

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

const handleImageError = () => {
  currentKaomoji.value = panickingKaomojis[Math.floor(Math.random() * panickingKaomojis.length)];
  isImageMovedTooFast.value = true;
};
</script>

<template>
  <div class="main-view">
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
            <p class="identified-text" v-if="resultData.name === 'Original' || resultData.fandom === '.original'">
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
              :title="resultData.dest"
              @mouseup="copyPathToClipboard"
              @contextmenu.prevent
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

    <transition name="toast-pop">
      <div v-if="isSlowNetwork" class="slow-network-banner unselectable">
        <ClockIcon :size="20" class="pulse-icon banner-icon" />
        <span>The IQDB servers are handling a lot of orders right now. Please be patient, your sauce will be out shortly.</span>
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
  justify-content: center; 
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
  border-radius: 16px; 
  padding: 40px 20px; 
  text-align: center; 
  box-shadow: 0 8px 30px rgba(0,0,0,0.3); 
  border: 1px dashed var(--bg-surface-elevated); 
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

/* -- PROCESSING CARD (Shared wrapper) -- */
.processing-card { 
  background: var(--bg-surface); 
  border-radius: 12px; 
  box-shadow: 0 8px 30px rgba(0,0,0,0.5); 
  border: 1px solid var(--bg-surface-elevated); 
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

.slow-network-banner {
  position: fixed;
  top: 80px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 100;
  
  width: calc(100% - 20px);
  box-sizing: border-box;
  
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
  background: rgba(245, 197, 66, 0.15);
  color: #f5c542;
  padding: 12px 18px;
  
  border-radius: 12px; 
  border: 1px dashed rgba(245, 197, 66, 0.3);
  backdrop-filter: blur(8px); 
  
  text-align: center;
  font-size: 0.75rem;
  font-weight: 700;
  line-height: 1.5;
}

.banner-icon {
  flex-shrink: 0;
}

.toast-pop-enter-active, 
.toast-pop-leave-active {
  transition: opacity 0.3s ease, transform 0.4s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}

.toast-pop-enter-from, 
.toast-pop-leave-to {
  opacity: 0;
  transform: translate(-50%, -20px); 
}

.pulse-icon {
  animation: pulse-anim 2s infinite;
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