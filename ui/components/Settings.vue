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
import { ref, watch, onMounted, computed, nextTick } from 'vue';
import draggable from 'vuedraggable';
import { 
  GripVerticalIcon, 
  CornerDownRightIcon, 
  InfoIcon, 
  AlertTriangleIcon 
} from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { open, ask } from '@tauri-apps/plugin-dialog';
import { enable, disable, isEnabled } from '@tauri-apps/plugin-autostart';
import { openUrl } from '@tauri-apps/plugin-opener';

import { isPermanentScan, confidenceThreshold, autoUpdateEnabled, isProcessing } from '../store';

// ---------------------------------*
// ---- META & INTERNAL STATE ------*
// ---------------------------------*

const appVersion = ref('1.0.0');
const startYear = 2026;
const currentYear = new Date().getFullYear();
const copyrightYears = currentYear > startYear ? `${startYear} - ${currentYear}` : `${startYear}`;

const isDataLoaded = ref(false);
const hiddenServices = ref({});

// ---------------------------------*
// ---- CONFIGURATION STATE --------*
// ---------------------------------*

// -- System Settings --
const autostart = ref(false);
const flags = ref({ 
  allowResizing: true,
  allowShrinking: true,
  startMinimized: false,
  allowImageConversion: true,
  listDupes: true,
  applyModsToSaved: false
});

// -- Folder Hierarchy --
const activeHierarchy = ref([
  { id: 1, name: 'Fandom' },
  { id: 2, name: 'Character' }
]);
const availableBlocks = ref([
  { id: 3, name: 'Artist' },
  { id: 4, name: 'Year' },
  { id: 5, name: 'Rating (SFW/NSFW)' }
]);

// -- File Handling & Directories --
const renameBehavior = ref('site_id');
const duplicateBehavior = ref('rename_copy');
const outputFolder = ref('');
const originalFolder = ref('.original');
const invalidFolder = ref('.invalid');
const blacklist = ref('');
const downloadsFolder = ref('.downloads');

// ---------------------------------*
// ---- COMPUTED PROPERTIES --------*
// ---------------------------------*

/**
 * Dynamically assigns a color class to the UI based on the strictness of the current confidence threshold slider.
 */
const thresholdColor = computed(() => {
  const val = confidenceThreshold.value;
  if (val < 66 || val > 90) return 'val-red';
  if ((val >= 66 && val <= 75) || (val >= 86 && val <= 90)) return 'val-yellow';
  return 'val-green';
});

/**
 * Generates contextual warning messages if the user sets the IQDB 
 * confidence threshold dangerously low (false positives) or high (false negatives).
 */
const thresholdWarning = computed(() => {
  const val = confidenceThreshold.value;
  
  if (val < 66) return "You might risk false positives. Unrelated images could be grouped together.";
  if (val > 90) return "You might risk false negatives. Because of compression and watermarks, even identical images can score below 90%.";
  
  return null; 
});

// ---------------------------------*
// ---- LIFECYCLE & WATCHERS -------*
// ---------------------------------*

onMounted(async () => {
  // 1. Fetch saved config from the Rust backend
  try {
    const config: any = await invoke('get_config');
    hiddenServices.value = config.services || {};

    if (config.flags) flags.value = { ...flags.value, ...config.flags };
    if (config.active_hierarchy && config.active_hierarchy.length > 0) activeHierarchy.value = config.active_hierarchy;
    if (config.available_blocks && config.available_blocks.length > 0) availableBlocks.value = config.available_blocks;

    renameBehavior.value = config.rename_behavior || 'site_id';
    duplicateBehavior.value = config.duplicate_behavior || 'rename_copy';
    confidenceThreshold.value = config.confidence_threshold || 80;

    if (config.flags && typeof config.flags.autoUpdateEnabled === 'boolean') {
      autoUpdateEnabled.value = config.flags.autoUpdateEnabled;
    }

    outputFolder.value = config.output_folder || '';
    originalFolder.value = config.original_folder || '.original';
    invalidFolder.value = config.invalid_folder || '.invalid';
    blacklist.value = config.blacklist || '';
    autostart.value = await isEnabled();
    downloadsFolder.value = config.downloads_folder || '.downloads';

    // Wait for Vue to apply the loaded data before enabling the auto-save watcher
    await nextTick();
    isDataLoaded.value = true;

  } catch (e) {
    console.error("Failed to load config, using defaults", e);
  }

  // 2. Fetch the compiled app version for the footer
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    console.warn("Could not fetch app version", e);
  }
});

/**
 * Deep Watcher: Automatically saves the configuration to disk (via Rust) 
 * whenever any settings reactive variable changes.
 */
watch(
  [
    isPermanentScan, autoUpdateEnabled, flags, activeHierarchy, availableBlocks, renameBehavior, 
    duplicateBehavior, confidenceThreshold, outputFolder, originalFolder, 
    invalidFolder, downloadsFolder, blacklist, autostart
  ], 
  async () => {
    // Prevent saving default states before the actual config is loaded
    if (!isDataLoaded.value) return;

    const payloadFlags = {
      ...flags.value,
      isPermanentScan: isPermanentScan.value,
      autoUpdateEnabled: autoUpdateEnabled.value,
      runOnBoot: autostart.value
    };

    const payload = {
      services: hiddenServices.value,
      flags: payloadFlags,
      active_hierarchy: activeHierarchy.value,
      available_blocks: availableBlocks.value,
      rename_behavior: renameBehavior.value,
      duplicate_behavior: duplicateBehavior.value,
      confidence_threshold: confidenceThreshold.value,
      output_folder: outputFolder.value,
      original_folder: originalFolder.value,
      invalid_folder: invalidFolder.value,
      downloads_folder: downloadsFolder.value,
      blacklist: blacklist.value,
    };
    
    try {
      await invoke('save_config', { config: payload });
    } catch (e) {
      console.error("Failed to save config", e);
    }
}, { deep: true });

/**
 * Syncs the frontend Autostart checkbox with the actual Host OS startup registry.
 */
watch(autostart, async (newVal) => {
  if (!isDataLoaded.value) return;

  try {
    if (newVal) await enable();
    else await disable();
  } catch (e) {
    console.error("Failed to toggle autostart in OS:", e);
  }
});

// ---------------------------------*
// ---- METHODS --------------------*
// ---------------------------------*

/**
 * Safely opens an external website URL in the user's default native web browser.
 * Uses a standard window.open fallback if the Tauri opener fails.
 * * @param url - The absolute URL to open.
 */
const openExternalLink = async (url: string) => {
  try {
    await openUrl(url);
  } catch (e) {
    window.open(url, '_blank');
  }
};

/**
 * Spawns the native OS directory selection dialog to pick an output folder.
 */
const pickOutputFolder = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select Output Directory'
    });

    if (selected !== null) {
      outputFolder.value = selected as string;
    }
  } catch (error) {
    console.error("Failed to open dialog:", error);
  }
};

/**
 * Intercepts changes to the Duplicate Behavior dropdown. 
 * If the user selects the "Delete" option, it spawns a native OS confirmation dialog to ensure
 * they understand it bypasses the recycle bin.
 * * @param e - The native select change event.
 */
const handleDuplicateChange = async (e: Event) => {
  const target = e.target as HTMLSelectElement;
  const newVal = target.value;

  if (newVal === 'delete') {
    let isConfirmed = false;
    try {
      isConfirmed = await ask(
        'Are you sure you want to delete files when encountering a duplicate? In rare cases, this could result in the deletion of false matches. This action bypasses the recycle bin, deleted images cannot be brought back.\n\nAre you sure you want to proceed?', 
        { title: 'Confirm Setting', kind: 'warning' }
      );
    } catch (err) {
      isConfirmed = window.confirm('Are you sure you want to delete files when encountering a duplicate? In rare cases, this could result in the deletion of false matches. This action bypasses the recycle bin, deleted images cannot be brought back.\n\nAre you sure you want to proceed?');
    }
    
    if (!isConfirmed) {
      target.value = duplicateBehavior.value; // Revert the UI dropdown
      return; // Safely aborts without saving
    }
  }
  
  // Update the variable (This triggers the deep watcher to save)
  duplicateBehavior.value = newVal; 
};

/**
 * Intercepts changes to the Apply Modifications checkbox.
 * Spawns a warning dialog when turning this feature ON, as it permanently alters source files with compressed/shrunk versions.
 * * @param e - The native checkbox change event.
 */
const handleApplyModsChange = async (e: Event) => {
  const target = e.target as HTMLInputElement;

  if (target.checked) {
    let isConfirmed = false;
    try {
      isConfirmed = await ask(
        // [TODO] Low prio: Would be cool if this error message read the current state of the settings and adjusted the message accordingly.
        'This will permanently alter the resolution, file size, and format of your original images (depending of your activated settings) when they are sorted.\n\nAre you sure you want to proceed?', 
        { title: 'Confirm Setting', kind: 'warning' }
      );
    } catch (err) {
      isConfirmed = window.confirm('This will permanently alter the resolution, file size, and format of your original images (depending of your activated settings) when they are sorted.\n\nAre you sure you want to proceed?');
    }
    
    if (!isConfirmed) {
      target.checked = false; // Revert the UI checkbox
      return; // Safely aborts without saving
    }
  }
  
  // Needed to trigger the watcher to save
  flags.value.applyModsToSaved = target.checked; 
};
</script>

<template>
  <div class="settings-view">
    
    <div class="setting-group">
      <h4>System Settings</h4>
      
      <label class="checkbox-label">
        <input type="checkbox" v-model="autostart" /> Start on system boot
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Launch SauceBottle in the background automatically when your computer starts.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="flags.startMinimized" /> Start minimized
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Launch SauceBottle quietly in the system tray without opening the main window.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="isPermanentScan" /> Process input directory automatically
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip edge-right">Continuously process the /input folder for new drops without needing to click the ▶︎ button.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="autoUpdateEnabled" /> Enable checking for new updates
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip edge-right">Check for new versions of SauceBottle on startup.</span>
        </div>
      </label>
      
      <div class="divider"></div>
      
      <label class="checkbox-label">
        <input type="checkbox" v-model="flags.allowResizing" /> Allow Image Resizing
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Scale down extremely large image dimensions to stay within API limits.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="flags.allowShrinking" /> Allow File Shrinking (>8MB)
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip edge-right">Compress images that exceed IQDB's strict 8MB file size limit.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="flags.allowImageConversion" /> Allow Image Conversion
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Automatically convert unsupported formats (like WEBP) to JPEG before lookup.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" v-model="flags.listDupes" /> List Duplicates in Logs
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Record skipped or renamed duplicates in the console for troubleshooting.</span>
        </div>
      </label>
      <label class="checkbox-label">
        <input type="checkbox" :checked="flags.applyModsToSaved" @change="handleApplyModsChange" /> 
        Apply modifications to saved file
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip edge-right">Permanently overwrites the sorted file with the resized/shrunk version used for the search. Applies only on successful matches!</span>
        </div>
      </label>
    </div>

    <div class="setting-group">
      <h4>Folder Hierarchy</h4>
      <p class="hint">Drag blocks between lists. Active blocks define how folders are nested.</p>
      
      <div class="hierarchy-container active-zone">
        <div class="zone-label">Active Structure</div>
        <draggable 
          v-model="activeHierarchy" 
          :group="{ name: 'hierarchy', pull: true, put: true }"
          item-key="id" 
          tag="div"
          class="drag-list" 
          :animation="200"
          ghost-class="ghost-item"
          :force-fallback="true">
          
          <template #item="{ element, index }">
            <div class="drag-item active-item" :style="{ marginLeft: `${index * 16}px` }">
              <CornerDownRightIcon v-if="index > 0" :size="14" class="tree-icon" />
              <GripVerticalIcon :size="16" class="drag-handle" /> 
              {{ element.name }}
            </div>
          </template>

          <template #footer>
            <div v-if="activeHierarchy.length === 0" class="empty-state">
              Drop blocks here to build path
            </div>
          </template>
        </draggable>
      </div>

      <div class="hierarchy-container">
        <div class="zone-label">Available Blocks</div>
        <draggable 
          v-model="availableBlocks" 
          :group="{ name: 'hierarchy', pull: true, put: true }"
          item-key="id" 
          tag="div"
          class="drag-list available-pool"
          :animation="200"
          ghost-class="ghost-item"
          :force-fallback="true">
          
          <template #item="{ element }">
            <div class="drag-item inactive-item">
              <GripVerticalIcon :size="16" class="drag-handle" /> 
              {{ element.name }}
            </div>
          </template>
        </draggable>
      </div>
    </div>

    <div class="setting-group">
      <h4>File Handling</h4>
      
      <div class="label-with-info">
        <label class="input-label">Rename Processed Files</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Dictates the final filename of a successfully matched image.</span>
        </div>
      </div>
      <select v-model="renameBehavior" class="text-input">
        <option value="original">Keep Original Name</option>
        <option value="site_id">Site Initial + ID (e.g., D1234.jpg)</option>
        <option value="random_id">Random ID (UUID)</option>
      </select>

      <div class="label-with-info">
        <label class="input-label">Duplicate Post Behavior</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">What to do if the exact same identified image already exists in the destination folder.</span>
        </div>
      </div>
      <select :value="duplicateBehavior" @change="handleDuplicateChange" class="text-input">
        <option value="rename_copy">Rename (append _copy)</option>
        <option value="move_folder_root">Move to duplicates folder (@root)</option>
        <option value="move_folder_deep">Move to duplicates folder (@deep)</option>
        <option value="delete" class="danger-option">Delete File</option>
      </select>

      <div class="label-with-info">
        <label class="input-label">Invalid Match Threshold</label>
        
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">The minimum similarity percentage from IQDB required to trust the result and move the file.</span>
        </div>

        <div v-if="thresholdWarning" class="info-wrapper warning-wrapper" style="margin-left: 8px;">
          <AlertTriangleIcon :size="16" class="warning-icon" :class="thresholdColor" />
          <span class="tooltip" :class="thresholdColor + '-border'">{{ thresholdWarning }}</span>
        </div>
      </div>
      
      <div class="threshold-row">
        <input type="range" min="0" max="100" v-model.number="confidenceThreshold" class="threshold-slider visual-track" />
        <span class="threshold-value" :class="thresholdColor">{{ confidenceThreshold }}%</span>
      </div>
      
      <div class="threshold-labels">
        <span>Permissive</span>
        <span>Strict</span>
      </div>
    </div>

    <div class="setting-group">
      <h4>Directories & Filters</h4>

      <div class="label-with-info">
        <label class="input-label">Output Folder</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">The main directory where processed images and subfolders will be generated.</span>
        </div>
      </div>
      <div class="path-input-row">
        <input type="text" class="text-input" v-model="outputFolder" placeholder="/path/to/output" />
        <button class="browse-btn" title="Browse" @click="pickOutputFolder">📁</button>
      </div>

      <div class="label-with-info">
        <label class="input-label">Original Artwork Folder</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">The folder name to use when the image is tagged as 'original' instead of a fandom.</span>
        </div>
      </div>
      <input type="text" class="text-input" v-model="originalFolder" />
      
      <div class="label-with-info">
        <label class="input-label">Invalid Match Folder</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Where to dump images that don't meet the confidence threshold or encounter an error.</span>
        </div>
      </div>
      <input type="text" class="text-input" v-model="invalidFolder" />

      <div class="label-with-info">
        <label class="input-label">Batch Downloads Folder</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Where to save bulk images downloaded via the Batch Downloader tab.</span>
        </div>
      </div>
      <input type="text" class="text-input" v-model="downloadsFolder" />

      <div class="label-with-info">
        <label class="input-label">Tag Blacklist (comma separated)</label>
        <div class="info-wrapper">
          <InfoIcon :size="14" class="info-icon" />
          <span class="tooltip">Tags to completely ignore when searching for Character, Fandom, or Artist names.</span>
        </div>
      </div>
      <input type="text" class="text-input" v-model="blacklist" placeholder="comic, sketch..." />
    </div>

    <div class="about-footer unselectable">
      <div class="credits">
        Made with ❤️ and <span class="coffee-emoji" title="I am more of a tea guy tbh">☕</span> by 
        <button class="link-btn" @click="openExternalLink('https://github.com/GitThirteen')">Thirteen</button>
      </div>
      
      <div class="copyright">
        &copy; {{ copyrightYears }} SauceBottle <span style="opacity: 0.5; margin-left: 5px;">v{{ appVersion }}</span>
      </div>

      <div class="kofi-section">
        <p>If SauceBottle saves you some time, consider tossing a coin into the tip jar!<br>
        <span class="subtext">(Absolutely no pressure though~ 🍵)</span></p>
        
        <button class="kofi-btn" @click="openExternalLink('https://ko-fi.com/akaxiii')">
          Support on Ko-fi
        </button>
      </div>
    </div>

  </div>
</template>

<style scoped>
.settings-view { padding: 20px; display: flex; flex-direction: column; gap: 20px; }

.setting-group { background: var(--bg-surface); padding: 20px; border-radius: 12px; border: 1px solid var(--bg-surface-elevated); }
h4 { color: var(--text-primary); margin-bottom: 12px; font-weight: 800; font-size: 1rem; }
.hint { color: var(--text-secondary); font-size: 0.85rem; margin-bottom: 15px; margin-top: 0; }
.divider { height: 1px; background: var(--bg-surface-elevated); margin: 15px 0; }

/* Tooltip System */
.label-with-info { display: flex; align-items: center; margin-top: 15px; margin-bottom: 5px; }
.label-with-info h4 { margin: 0; }
.label-with-info .input-label { margin: 0; }

.info-wrapper { position: relative; display: inline-flex; align-items: center; margin-left: 6px; }
.info-icon { color: var(--text-tertiary); cursor: help; transition: color 0.2s; }
.info-icon:hover { color: var(--accent-primary); }

.tooltip { 
  visibility: hidden; 
  position: absolute; 
  bottom: 150%; 
  left: 50%; 
  transform: translateX(-50%); 
  background: var(--bg-surface-elevated); 
  color: var(--text-primary); 
  padding: 8px 12px; 
  border-radius: 6px; 
  font-size: 0.75rem; 
  font-weight: 600;
  width: max-content; 
  max-width: 220px; 
  text-align: center; 
  box-shadow: 0 4px 12px rgba(0,0,0,0.5); 
  border: 1px solid var(--text-tertiary); 
  z-index: 50; 
  opacity: 0; 
  transition: opacity 0.2s, visibility 0.2s; 
  pointer-events: none; 
}
.tooltip::after {
  content: '';
  position: absolute;
  top: 100%;
  left: 50%;
  margin-left: -5px;
  border-width: 5px;
  border-style: solid;
  border-color: var(--text-tertiary) transparent transparent transparent;
}
.info-wrapper:hover .tooltip { visibility: visible; opacity: 1; }

/* Form Controls */
.input-label { display: block; font-size: 0.85rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 5px; margin-top: 15px; }
.checkbox-label { margin-bottom: 8px; display: flex; align-items: center; gap: 6px; color: var(--text-primary); font-size: 0.9rem; font-weight: 600; cursor: pointer; width: max-content; }
.checkbox-label input[type="checkbox"] { accent-color: var(--accent-primary); width: 16px; height: 16px; margin-right: 4px;}

.path-input-row { display: flex; gap: 8px; align-items: center; }
.path-input-row .text-input { flex: 1; }
.browse-btn { flex-shrink: 0; padding: 10px 12px; background: var(--bg-base); border: 1px solid var(--bg-surface-elevated); border-radius: 6px; cursor: pointer; font-size: 1rem; transition: border-color 0.2s; }
.browse-btn:hover { border-color: var(--accent-primary); }

.text-input { width: 100%; padding: 10px 12px; background-color: var(--bg-base); border: 1px solid var(--bg-surface-elevated); color: var(--text-primary); border-radius: 6px; box-sizing: border-box; font-family: 'Nunito', sans-serif; transition: border-color 0.2s; }
.text-input:focus { outline: none; border-color: var(--accent-primary); }
.danger-option { color: #ff4d4d; font-weight: 800; }

/* Drag and Drop */
.hierarchy-container { background: var(--bg-base); border-radius: 8px; padding: 12px; margin-bottom: 15px; border: 1px solid var(--bg-surface-elevated); }
.zone-label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 1px; color: var(--text-tertiary); margin-bottom: 10px; font-weight: 800; }

.drag-list { display: flex; flex-direction: column; gap: 8px; min-height: 70px; padding: 5px; border-radius: 6px; background: rgba(0,0,0,0.2); }
.ghost-item { opacity: 0.4; background: var(--bg-surface-elevated); border: 2px dashed var(--accent-primary); }
.drag-item { padding: 10px 12px; border-radius: 6px; display: flex; align-items: center; gap: 10px; cursor: grab; font-weight: 600; font-size: 0.9rem; transition: background 0.2s; background: var(--bg-surface-elevated); user-select: none; }
.drag-item:active { cursor: grabbing; }
.drag-handle { color: var(--text-tertiary); }

.active-item { border-left: 3px solid var(--accent-primary); }
.inactive-item { background: transparent; border: 1px dashed var(--text-tertiary); color: var(--text-secondary); }
.tree-icon { color: var(--accent-primary); margin-right: -4px; opacity: 0.7; }
.empty-state { text-align: center; font-size: 0.85rem; color: var(--text-tertiary); padding: 10px; border: 1px dashed var(--bg-surface-elevated); border-radius: 6px; margin-top: 5px; }

:global(.sortable-fallback) { opacity: 1 !important; background: var(--bg-surface-elevated) !important; border-radius: 6px; pointer-events: none; }

/* Threshold Slider */
.threshold-row { display: flex; align-items: center; gap: 15px; margin-top: 8px; }

.threshold-slider { flex: 1; -webkit-appearance: none; appearance: none; outline: none; cursor: pointer; }

.threshold-slider.visual-track {
  height: 6px;
  border-radius: 3px;
  background: var(--bg-surface-elevated);
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.3);
}

.threshold-slider::-webkit-slider-thumb { -webkit-appearance: none; width: 18px; height: 18px; border-radius: 50%; background: var(--accent-primary); box-shadow: 0 2px 8px rgba(255,75,43,0.4); cursor: pointer; transition: transform 0.15s; }
.threshold-slider::-webkit-slider-thumb:hover { transform: scale(1.2); }

.threshold-value { font-size: 0.95rem; font-weight: 800; min-width: 45px; text-align: right; }

.val-red { color: #ff4d4d; }
.val-yellow { color: #f5c542; }
.val-green { color: var(--success, #00e676); }

.threshold-labels {
  display: flex;
  justify-content: space-between;
  margin-top: 8px;
  font-size: 0.7rem;
  font-weight: 800;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 1px;
}

.warning-icon { cursor: help; animation: pop 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275); }
.tooltip.edge-right {
  left: auto;
  right: -10px;
  transform: none;
}

.tooltip.edge-right::after {
  left: auto;
  right: 14px;
  margin-left: 0;
}

.tooltip.edge-left {
  left: -10px;
  transform: none;
}
.tooltip.edge-left::after {
  left: 14px;
  margin-left: 0;
}

.warning-wrapper .tooltip {
  border-width: 1px;
  border-style: solid;
  bottom: 130%; 
}

.warning-wrapper .tooltip.val-red-border { border-color: #ff4d4d; }
.warning-wrapper .tooltip.val-yellow-border { border-color: #f5c542; }

.settings-view { scrollbar-gutter: auto; }

/* About Footer */
.about-footer {
  margin-top: 20px;
  padding: 30px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  border-top: 1px dashed var(--bg-surface-elevated);
}

.credits {
  color: var(--text-secondary);
  font-size: 0.95rem;
  font-weight: 600;
  margin-bottom: 5px;
}

.coffee-emoji {
  cursor: help;
  display: inline-block;
  transition: transform 0.2s;
}
.coffee-emoji:hover {
  transform: scale(1.2) rotate(-10deg);
}

.link-btn {
  background: none;
  border: none;
  padding: 0;
  color: var(--accent-primary);
  font-family: inherit;
  font-size: inherit;
  font-weight: 800;
  cursor: pointer;
  text-decoration: underline;
  text-decoration-color: transparent;
  transition: text-decoration-color 0.2s;
}
.link-btn:hover {
  text-decoration-color: var(--accent-primary);
}

.copyright {
  color: var(--text-tertiary);
  font-size: 0.75rem;
  letter-spacing: 1px;
  margin-bottom: 25px;
}

/* Ko-fi Section */
.kofi-section {
  background: rgba(0, 0, 0, 0.15);
  padding: 20px 25px;
  border-radius: 12px;
  border: 1px solid var(--bg-surface-elevated);
  max-width: 400px;
}

.kofi-section p {
  color: var(--text-secondary);
  font-size: 0.85rem;
  line-height: 1.5;
  margin: 0 0 15px 0;
}

.kofi-section .subtext {
  font-size: 0.75rem;
  color: var(--text-tertiary);
  font-style: italic;
}

.kofi-btn {
  background: var(--bg-surface);
  color: var(--text-primary);
  border: 1px solid var(--bg-surface-elevated);
  padding: 8px 20px;
  border-radius: 999px;
  font-family: 'Nunito', sans-serif;
  font-weight: 800;
  font-size: 0.85rem;
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.kofi-btn:hover {
  border-color: #ff5e5b;
  color: #ff5e5b;
  background: rgba(255, 94, 91, 0.05);
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(255, 94, 91, 0.15);
}

.kofi-btn:active {
  transform: translateY(0);
}
</style>