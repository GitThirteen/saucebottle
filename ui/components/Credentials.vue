<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { 
  ExternalLinkIcon, 
  ShieldCheckIcon, 
  Trash2Icon, 
  Edit2Icon, 
  CheckCircle2Icon, 
  StarIcon, 
  InfoIcon 
} from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';

import { credentials, vaultStatus, refreshVaultStatus } from '../store';

// ---------------------------------*
// ---- LOCAL STATE ----------------*
// ---------------------------------*

/** Tracks which credential cards are currently in "edit mode" versus "locked mode". */
const isEditing = ref({ 
  danbooru: false, 
  gelbooru: false 
});

// ---------------------------------*
// ---- LIFECYCLE ------------------*
// ---------------------------------*

onMounted(async () => {
  // Check the OS vault immediately when the tab is opened to ensure the UI is accurate
  await refreshVaultStatus();
});

// ---------------------------------*
// ---- METHODS --------------------*
// ---------------------------------*

/**
 * Unlocks the credential card for editing and wipes any stale input data.
 * * @param service - The booru service to unlock ('danbooru' | 'gelbooru').
 */
const openEdit = (service: 'danbooru' | 'gelbooru') => {
  credentials.value[service] = { username: '', apiKey: '' };
  isEditing.value[service] = true;
};

/**
 * Stringifies the user's input and sends it to the Rust backend to be securely 
 * encrypted and stored in the host OS's native credential manager.
 * * @param service - The booru service to save ('danbooru' | 'gelbooru').
 */
const saveSecurely = async (service: 'danbooru' | 'gelbooru') => {
  try {
    const dataString = JSON.stringify(credentials.value[service]);
    await invoke('save_credential', { service, key: dataString });
    
    // Clear the plain-text inputs from memory immediately after saving
    credentials.value[service] = { username: '', apiKey: '' };
    isEditing.value[service] = false;
    
    // Update the global store to reflect the new saved state
    await refreshVaultStatus();
    
  } catch (e) {
    console.error(`Failed to save ${service} to vault`, e);
  }
};

/**
 * Prompts the user for confirmation, then asks the Rust backend to purge 
 * the saved credentials from the OS keyring.
 * * @param service - The booru service to delete ('danbooru' | 'gelbooru').
 */
const deleteSecurely = async (service: 'danbooru' | 'gelbooru') => {
  if (!confirm(`Are you sure you want to permanently delete your ${service} credentials?`)) return;
  
  try {
    await invoke('delete_credential', { service });
    
    credentials.value[service] = { username: '', apiKey: '' };
    isEditing.value[service] = false;
    
    await refreshVaultStatus();
  } catch(e) {
    console.error("Failed to delete", e);
  }
};

/**
 * Safely opens an external website URL in the user's default native web browser.
 * * @param url - The absolute URL to open.
 */
const openSite = async (url: string) => {
  await openUrl(url);
};
</script>

<template>
  <div class="credentials-view">
    
    <div class="security-banner">
      <ShieldCheckIcon :size="24" class="shield-icon" />
      <div class="banner-text">
        <h3>Your Data is Secure</h3>
        <p>API keys are encrypted and stored in your OS' native credential manager. They are never exposed.</p>
      </div>
    </div>

    <div class="info-banner">
      <InfoIcon :size="24" class="info-icon-banner" />
      <div class="banner-text">
        <h3>Yande.re Enabled by Default</h3>
        <p>SauceBottle natively uses Yande.re as a free fallback. Adding your own Danbooru or Gelbooru credentials below will significantly increase match success rates!</p>
      </div>
    </div>

    <div class="credential-card danbooru-card">
      <div class="recommended-corner-tab unselectable">
        <StarIcon :size="12" fill="currentColor" /> Recommended
      </div>

      <div class="card-header">
        <h4>Danbooru Credentials</h4>
        <button class="link-btn" @click="openSite('https://danbooru.donmai.us/profile')">
          Sign Up <ExternalLinkIcon :size="14" />
        </button>
      </div>
      
      <div v-if="vaultStatus.danbooru && !isEditing.danbooru" class="locked-state">
         <div class="locked-info"><CheckCircle2Icon :size="20"/> Saved</div>
         <div class="locked-actions">
            <button class="change-btn" @click="openEdit('danbooru')"><Edit2Icon :size="16"/> Change</button>
            <button class="trash-btn" @click="deleteSecurely('danbooru')" title="Delete Credentials"><Trash2Icon :size="18"/></button>
         </div>
      </div>

      <div v-else class="edit-state">
        <div class="input-group">
          <label>Username</label>
          <input type="text" class="text-input" v-model="credentials.danbooru.username" placeholder="e.g. SauceMaster99" />
        </div>
        
        <div class="input-group">
          <label>API Key</label>
          <input type="password" class="text-input" v-model="credentials.danbooru.apiKey" placeholder="Paste your API key here..." />
        </div>

        <button class="save-btn" @click="saveSecurely('danbooru')">Save to Vault</button>
        <button v-if="vaultStatus.danbooru" class="cancel-btn" @click="isEditing.danbooru = false">Cancel</button>
      </div>
    </div>

    <div class="credential-card gelbooru-card">
      <div class="card-header">
        <h4>Gelbooru Credentials</h4>
        <button class="link-btn" @click="openSite('https://gelbooru.com/index.php?page=account&s=options')">
          Sign Up <ExternalLinkIcon :size="14" />
        </button>
      </div>
      
      <div v-if="vaultStatus.gelbooru && !isEditing.gelbooru" class="locked-state">
         <div class="locked-info"><CheckCircle2Icon :size="20"/> Saved</div>
         <div class="locked-actions">
            <button class="change-btn" @click="openEdit('gelbooru')"><Edit2Icon :size="16"/> Change</button>
            <button class="trash-btn" @click="deleteSecurely('gelbooru')" title="Delete Credentials"><Trash2Icon :size="18"/></button>
         </div>
      </div>

      <div v-else class="edit-state">
        <div class="input-group">
          <label>User ID</label>
          <input type="text" class="text-input" v-model="credentials.gelbooru.username" placeholder="Optional for some queries" />
        </div>
        
        <div class="input-group">
          <label>API Key</label>
          <input type="password" class="text-input" v-model="credentials.gelbooru.apiKey" placeholder="Paste your API key here..." />
        </div>

        <button class="save-btn" @click="saveSecurely('gelbooru')">Save to Vault</button>
        <button v-if="vaultStatus.gelbooru" class="cancel-btn" @click="isEditing.gelbooru = false">Cancel</button>
      </div>
    </div>

  </div>
</template>

<style scoped>
.credentials-view { 
  padding: 20px; 
  display: flex; 
  flex-direction: column; 
  gap: 20px; 
}

/* -- Banners -- */
.security-banner { 
  display: flex; 
  align-items: center; 
  gap: 15px; 
  background: rgba(0, 230, 118, 0.1); 
  border: 1px solid rgba(0, 230, 118, 0.3); 
  padding: 15px; 
  border-radius: 8px; 
}
.shield-icon { 
  color: var(--success); 
  flex-shrink: 0; 
}
.banner-text h3 { 
  margin: 0 0 5px 0; 
  font-size: 0.95rem; 
  color: var(--success); 
}
.banner-text p { 
  margin: 0; 
  font-size: 0.8rem; 
  color: var(--text-secondary); 
  line-height: 1.4; 
}

.info-banner { 
  display: flex; 
  align-items: center; 
  gap: 15px; 
  background: rgba(126, 214, 223, 0.1); 
  border: 1px solid rgba(126, 214, 223, 0.3); 
  padding: 15px; 
  border-radius: 8px; 
}
.info-icon-banner { 
  color: #7ed6df; 
  flex-shrink: 0; 
}
.info-banner .banner-text h3 { 
  color: #7ed6df; 
}

/* -- Credential Cards -- */
.credential-card { 
  padding: 20px; 
  border-radius: 12px; 
  border: 2px dashed; 
  background: var(--bg-surface); 
  position: relative; 
  overflow: hidden; 
}
.danbooru-card { 
  border-color: #ff7979; 
  padding-top: 35px; 
}
.gelbooru-card { 
  border-color: #7ed6df; 
}

/* Card Radial Gradients */
.danbooru-card::before { 
  content: ''; 
  position: absolute; 
  top: 0; right: 0; 
  width: 150px; height: 150px; 
  background: radial-gradient(circle, rgba(255, 121, 121, 0.1) 0%, transparent 70%); 
  pointer-events: none; 
}
.gelbooru-card::before { 
  content: ''; 
  position: absolute; 
  top: 0; right: 0; 
  width: 150px; height: 150px; 
  background: radial-gradient(circle, rgba(126, 214, 223, 0.1) 0%, transparent 70%); 
  pointer-events: none; 
}

/* Corner Ribbons */
.recommended-corner-tab { 
  position: absolute; 
  top: 0; left: 0; 
  background: linear-gradient(135deg, #ff7979 0%, #ff4d4d 100%); 
  color: white; 
  padding: 6px 16px; 
  font-size: 0.65rem; 
  font-weight: 800; 
  text-transform: uppercase; 
  letter-spacing: 1px; 
  border-bottom-right-radius: 16px; 
  box-shadow: 2px 2px 15px rgba(255, 121, 121, 0.4); 
  display: flex; 
  align-items: center; 
  gap: 6px; 
  z-index: 10; 
}

/* -- Card Headers -- */
.card-header { 
  display: flex; 
  justify-content: space-between; 
  align-items: center; 
  margin-bottom: 15px; 
  border-bottom: 1px solid var(--bg-surface-elevated); 
  padding-bottom: 10px; 
}
.card-header h4 { 
  margin: 0; 
  font-size: 1.1rem; 
  color: var(--text-primary); 
}
.link-btn { 
  background: none; 
  border: none; 
  color: var(--text-secondary); 
  cursor: pointer; 
  display: flex; 
  align-items: center; 
  gap: 5px; 
  font-size: 0.8rem; 
  font-weight: 600; 
  transition: color 0.2s; 
}
.link-btn:hover { 
  color: var(--accent-primary); 
}

/* -- Inputs -- */
.input-group { margin-bottom: 15px; }
.input-group label { 
  display: block; 
  font-size: 0.8rem; 
  font-weight: 700; 
  color: var(--text-secondary); 
  margin-bottom: 5px; 
  text-transform: uppercase; 
  letter-spacing: 0.5px; 
}
.text-input { 
  width: 100%; 
  padding: 10px 12px; 
  background: var(--bg-base); 
  border: 1px solid var(--bg-surface-elevated); 
  color: var(--text-primary); 
  border-radius: 6px; 
  box-sizing: border-box; 
  font-family: monospace; 
  transition: border-color 0.2s; 
}
.text-input:focus { 
  outline: none; 
  border-color: var(--accent-primary); 
}

/* -- Buttons -- */
.save-btn { 
  width: 100%; 
  padding: 12px; 
  background: var(--bg-surface-elevated); 
  color: var(--text-primary); 
  border: 1px solid var(--bg-surface-elevated); 
  border-radius: 6px; 
  font-weight: 800; 
  cursor: pointer; 
  transition: all 0.2s; 
  margin-top: 5px; 
}
.save-btn:hover { 
  background: var(--accent-primary); 
  border-color: var(--accent-primary); 
  box-shadow: 0 4px 12px rgba(255, 75, 43, 0.3); 
}
.cancel-btn { 
  width: 100%; 
  padding: 12px; 
  background: transparent; 
  color: var(--text-secondary); 
  border: 1px dashed var(--bg-surface-elevated); 
  border-radius: 6px; 
  font-weight: 700; 
  cursor: pointer; 
  transition: all 0.2s; 
  margin-top: 10px; 
}
.cancel-btn:hover { 
  color: var(--text-primary); 
  border-color: var(--text-tertiary); 
}

/* -- Locked/Saved State UI -- */
.locked-state { 
  display: flex; 
  justify-content: space-between; 
  align-items: center; 
  background: rgba(0, 230, 118, 0.05); 
  border: 1px solid rgba(0, 230, 118, 0.2); 
  padding: 15px; 
  border-radius: 8px; 
  margin-top: 10px; 
}
.locked-info { 
  display: flex; 
  align-items: center; 
  gap: 8px; 
  color: var(--success); 
  font-weight: 700; 
  font-size: 0.9rem; 
}
.locked-actions { 
  display: flex; 
  gap: 8px; 
}
.change-btn { 
  display: flex; 
  align-items: center; 
  gap: 6px; 
  padding: 8px 12px; 
  background: var(--bg-surface-elevated); 
  border: 1px solid var(--bg-surface-elevated); 
  color: var(--text-primary); 
  border-radius: 6px; 
  cursor: pointer; 
  font-weight: 700; 
  font-size: 0.8rem; 
  transition: all 0.2s; 
}
.change-btn:hover { 
  border-color: var(--accent-primary); 
  color: var(--accent-primary); 
}
.trash-btn { 
  display: flex; 
  align-items: center; 
  justify-content: center; 
  padding: 8px; 
  background: rgba(255, 77, 77, 0.1); 
  border: 1px solid rgba(255, 77, 77, 0.3); 
  color: #ff4d4d; 
  border-radius: 6px; 
  cursor: pointer; 
  transition: all 0.2s; 
}
.trash-btn:hover { 
  background: #ff4d4d; 
  color: white; 
}
</style>