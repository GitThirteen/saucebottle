<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { DownloadCloudIcon, StopCircleIcon, FolderOpenIcon } from 'lucide-vue-next';

import { vaultStatus } from '../store';
import { logger } from '../logger';

// ---------------------------------*
// ---- LOCAL STATE ----------------*
// ---------------------------------*

const selectedService = ref('Yande.re');
const tags = ref('');
const startPage = ref(1);
const endPage = ref<number | ''>('');
const isDownloading = ref(false);
const downloadsFolder = ref('.downloads');

// ---------------------------------*
// ---- COMPUTED -------------------*
// ---------------------------------*

/**
 * List of available services. Disables options in the dropdown if the user hasn't configured the required API keys.
 */
const availableServices = computed(() => [
  { id: 'Danbooru', name: 'Danbooru', enabled: vaultStatus.value.danbooru },
  { id: 'Gelbooru', name: 'Gelbooru', enabled: vaultStatus.value.gelbooru },
  { id: 'Yande.re', name: 'Yande.re (No Auth Required)', enabled: true }
]);

// ---------------------------------*
// ---- LIFECYCLE ------------------*
// ---------------------------------*

onMounted(async () => {
  // Fetch the custom downloads folder name from the user's config
  try {
    const config: any = await invoke('get_config');
    if (config.downloads_folder) downloadsFolder.value = config.downloads_folder;
  } catch (e) {
    console.warn("Could not load config for DL view");
  }
});

// ---------------------------------*
// ---- METHODS --------------------*
// ---------------------------------*

/**
 * It's a sleep function.
 */
const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

/**
 * Asks the Rust backend to open the OS native file explorer directly to the downloads folder.
 */
const openDlFolder = async () => {
  try { 
    await invoke('open_system_folder', { folderTarget: 'downloads' }); 
  } catch (e) { 
    console.error("Failed to open folder", e); 
  }
};

/**
 * Initiates the batch download sequence. 
 * Paginates through the selected Booru API, extracting image URLs, and sending them 
 * to the Rust backend to be saved to the disk. Respects the global `isDownloading` flag 
 * so the process can be safely aborted midway.
 */
const startDownload = async () => {
  if (!tags.value.trim()) return alert("Please enter at least one tag.");
  
  isDownloading.value = true;
  logger.info(`Starting batch download from ${selectedService.value} for tags: [${tags.value}]`);
  
  let page = startPage.value;
  const end = typeof endPage.value === 'number' && endPage.value > 0 ? endPage.value : Infinity;
  
  while (page <= end && isDownloading.value) {
    logger.info(`Fetching Page ${page}...`);
    try {
      // 1. Fetch the JSON payload for the current page
      const images: any[] = await invoke('fetch_booru_page', { 
        service: selectedService.value, 
        tags: tags.value, 
        page 
      });

      // The page is empty (we reached the end of the search results)
      if (images.length === 0) {
        logger.info(`No more images found on page ${page}. Download complete!`);
        break;
      }

      logger.info(`Found ${images.length} posts. Downloading...`);
      
      // 2. Loop through every image on the page and invoke the Rust downloader
      for (const img of images) {
        if (!isDownloading.value) break; // Check if user clicked Abort
        
        const filename = `${img.id}.${img.ext}`;
        
        try {
          await invoke('download_image', { url: img.url, filename });
          logger.success(`Downloaded ${filename}`);
        } catch (e) {
          logger.error(`Failed to download ${filename}: ${e}`);
        }
      }
      
      // 3. Prepare for the next page, respecting the rate limit
      if (isDownloading.value) {
        await sleep(1000); // Lil' break so the sites don't hate us
        page++;
      }
    } catch (err) {
      logger.error(`API Error on page ${page}: ${err}`);
      break;
    }
  }
  
  if (!isDownloading.value) logger.warn("Download manually aborted.");
  else logger.success("Batch download finished.");
  
  isDownloading.value = false;
};

/**
 * Safely interrupts an active batch download.
 * The loop in `startDownload` checks this flag before starting the next file/page.
 */
const stopDownload = () => { 
  isDownloading.value = false; 
};
</script>

<template>
  <div class="download-view">
    <div class="setting-group">
      <div class="header-row">
        <DownloadCloudIcon class="header-icon" :size="24" />
        <h2>Batch Downloader</h2>
      </div>
      <p class="hint">Fetch and save bulk image sets directly to your /{{ downloadsFolder }} folder.</p>

      <label class="input-label">Select Source</label>
      <select v-model="selectedService" class="text-input" :disabled="isDownloading">
        <option 
          v-for="srv in availableServices" 
          :key="srv.id" 
          :value="srv.id" 
          :disabled="!srv.enabled"
        >
          {{ srv.name }} {{ !srv.enabled ? '(Key Required)' : '' }}
        </option>
      </select>

      <label class="input-label">Tags (comma separated)</label>
      <input type="text" class="text-input" v-model="tags" placeholder="arknights, official_art" :disabled="isDownloading" />

      <div class="page-row">
        <div class="page-col">
          <label class="input-label">Start Page</label>
          <input type="number" class="text-input" v-model.number="startPage" min="1" :disabled="isDownloading" />
        </div>
        <div class="page-col">
          <label class="input-label">End Page</label>
          <input type="number" class="text-input" v-model.number="endPage" min="1" :disabled="isDownloading" />
          <span class="input-subtext">(Leave empty for ALL)</span>
        </div>
      </div>

      <div class="action-row">
        <button v-if="!isDownloading" class="btn-primary" @click="startDownload">
          <DownloadCloudIcon :size="18" /> Download
        </button>
        <button v-else class="btn-danger" @click="stopDownload">
          <StopCircleIcon :size="18" /> Abort Download
        </button>

        <button class="btn-secondary" @click="openDlFolder" title="Open Downloads Folder">
          <FolderOpenIcon :size="18" /> Open Folder
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.download-view { 
  padding: 20px; 
  display: flex; 
  flex-direction: column; 
  gap: 20px; 
}

.setting-group { 
  background: var(--bg-surface); 
  padding: 25px; 
  border-radius: 12px; 
  border: 1px solid var(--bg-surface-elevated); 
}

.header-row { 
  display: flex; 
  align-items: center; 
  gap: 10px; 
  margin-bottom: 5px; 
}

.header-icon { 
  color: var(--accent-primary); 
}

h2 { 
  color: var(--text-primary); 
  margin: 0; 
  font-weight: 800; 
  font-size: 1.2rem; 
}

.hint { 
  color: var(--text-secondary); 
  font-size: 0.85rem; 
  margin-bottom: 20px; 
  margin-top: 0; 
}

/* -- Inputs -- */
.input-label { 
  display: block; 
  font-size: 0.85rem; 
  font-weight: 600; 
  color: var(--text-secondary); 
  margin-bottom: 5px; 
  margin-top: 15px; 
}

.text-input { 
  width: 100%; 
  padding: 10px 12px; 
  background-color: var(--bg-base); 
  border: 1px solid var(--bg-surface-elevated); 
  color: var(--text-primary); 
  border-radius: 6px; 
  box-sizing: border-box; 
  font-family: 'Nunito', sans-serif; 
  transition: border-color 0.2s; 
}

.text-input:focus { 
  outline: none; 
  border-color: var(--accent-primary); 
}

.text-input:disabled { 
  opacity: 0.5; 
  cursor: not-allowed; 
}

.page-row { 
  display: flex; 
  gap: 15px; 
}

.page-col { 
  flex: 1; 
}

.input-subtext { 
  display: block; 
  font-size: 0.75rem; 
  color: var(--text-tertiary); 
  margin-top: 6px; 
  font-weight: 600; 
}

/* -- Actions & Buttons -- */
.action-row { 
  margin-top: 25px; 
  display: flex; 
  justify-content: center; 
  gap: 15px; 
}

button { 
  display: flex; 
  align-items: center; 
  gap: 8px; 
  padding: 12px 20px; 
  border-radius: 8px; 
  font-weight: 800; 
  cursor: pointer; 
  transition: transform 0.15s, opacity 0.15s, background 0.15s, border-color 0.15s; 
  border: none; 
  font-family: inherit; 
}

button:active { 
  transform: scale(0.95); 
}

.btn-primary { 
  background: var(--accent-gradient); 
  color: white; 
  box-shadow: 0 4px 12px rgba(255, 75, 43, 0.3); 
}

.btn-danger { 
  background: rgba(255, 77, 77, 0.15); 
  color: #ff4d4d; 
  border: 1px solid rgba(255, 77, 77, 0.4); 
}

.btn-secondary { 
  background: var(--bg-surface-elevated); 
  color: var(--text-secondary); 
  border: 1px solid transparent; 
}

.btn-secondary:hover { 
  color: var(--text-primary); 
  border-color: var(--text-tertiary); 
  background: rgba(255, 255, 255, 0.05); 
}

.icon-btn { 
  background: transparent; 
  border: 1px solid var(--bg-surface-elevated); 
  color: var(--text-tertiary); 
  padding: 6px; 
  border-radius: 6px; 
  cursor: pointer; 
  transition: all 0.2s; 
  margin-left: auto; 
  display: flex; 
  align-items: center; 
  justify-content: center; 
}

.icon-btn:hover { 
  background: var(--bg-surface-elevated); 
  color: var(--accent-primary); 
}
</style>