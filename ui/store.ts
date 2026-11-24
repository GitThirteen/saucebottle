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

import { computed, ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { logger, type LogDetail } from './logger';

// ---------------------------------*
// ---- TYPES & CONSTANTS ----------*
// ---------------------------------*

interface UIEvent { 
    type: 'proc' | 'succ' | 'fail'; 
    payload: any; 
}

const sauceQuotes = [
    { text: "Squeezing the Bottle...",         weight: 15, rare: false },
    { text: "Lost in the sauce...",            weight: 10, rare: false },
    { text: "Obtaining the secret formula...", weight: 4,  rare: false },
    { text: "Consulting the sauce boss...",    weight: 10, rare: false },
    { text: "Identifying flavor profile...",   weight: 5,  rare: false },
    { text: "Deglazing the pixels...",         weight: 15, rare: false },
    { text: "Looking for Sauce...",            weight: 40, rare: false },
    { text: "Where is the lamb sauce!?",       weight: 1,  rare: true  },
];

// ---------------------------------*
// ---- REACTIVE STATE -------------*
// ---------------------------------*

// -- App & Processing State --
export const appState = ref<'welcome' | 'processing' | 'result' | 'error' | 'updating'>('welcome');
export const autoUpdateEnabled = ref(true);
export const isPermanentScan = ref(true);
export const isProcessing = ref(true);
export const confidenceThreshold = ref(80);
/** Goes up each time the settings were saved, for views that depend on them. */
export const settingsSavedTick = ref(0);

// -- Updater State --
export const updateProgress = ref(0);
export const updateStatus = ref('Starting download...');

// -- UI Variables --
export const loadingMessage = ref('Looking For Sauce...');
export const isRareMessage = ref(false);
export const activeImage = ref('');
export const errorMessage = ref('');
export const resultData = ref({ 
    name: '', fandom: '', artist: '', service: '', dest: '', conf: 0, isOriginalWork: false
});

// -- Queue Tracking --
export const queueCount = ref(0);
export const offlineQueueCount = ref(0);

// -- Credentials State --
export const credentials = ref({
    danbooru: { username: '', apiKey: '' },
    gelbooru: { username: '', apiKey: '' }
});
export const vaultStatus = ref({
    danbooru: false,
    gelbooru: false
});

// ---------------------------------*
// ---- INTERNAL STATE -------------*
// ---------------------------------*

const uiQueue: UIEvent[] = [];
let isUILoopRunning = false;
let lastProcStartTime = 0;
let isInitialized = false;
let returnToWelcomeTimer: number | undefined;

/** How long a finished result stays up (for Copy path / Open folder) when nothing is left to process. */
const RESULT_LINGER_MS = 8000;

/** Size info of the images being processed, by file name. Used for their log rows afterwards. */
const imageInfoByFile = new Map<string, string>();

/** Builds a `LogDetail` list from label/value pairs and skips the ones without a value. */
const toDetails = (pairs: [string, string | undefined | null][]): LogDetail[] =>
    pairs.filter((pair): pair is [string, string] => !!pair[1]).map(([label, value]) => ({ label, value }));

// ---------------------------------*
// ---- COMPUTED & WATCHERS --------*
// ---------------------------------*

/** Returns true if at least one Booru service has valid credentials. */
export const hasCredentials = computed(() => vaultStatus.value.danbooru || vaultStatus.value.gelbooru);

// ---------------------------------*
// ---- UTILITIES ------------------*
// ---------------------------------*

/**
 * It's a sleep function, what do you think it does?
 */
const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

// ---------------------------------*
// ---- CORE ACTIONS ---------------*
// ---------------------------------*

/**
 * Pings the Rust backend to check the actual file system for unprocessed images.
 * Updates the `offlineQueueCount` to reflect how many files are waiting in the `.input` folder.
 */
export const syncOfflineQueue = async () => {
    try {
        offlineQueueCount.value = await invoke('check_input_folder');
    } catch (e) {
        console.warn("Could not sync offline queue", e);
    }
};

/**
 * Queries the native OS Credential Manager (via Rust) to see if valid API keys
 * are stored for the supported booru services. Updates the `vaultStatus` reactivity.
 */
export const refreshVaultStatus = async () => {
    try {
        const d: string = await invoke('get_credential', { service: 'danbooru' });
        try {
            const parsed = JSON.parse(d);
            vaultStatus.value.danbooru = typeof parsed.username === 'string' && parsed.username.trim() !== ''
                                      && typeof parsed.apiKey === 'string'  && parsed.apiKey.trim()  !== '';
        } catch { vaultStatus.value.danbooru = false; }

        const g: string = await invoke('get_credential', { service: 'gelbooru' });
        try {
            const parsed = JSON.parse(g);
            vaultStatus.value.gelbooru = typeof parsed.username === 'string' && parsed.username.trim() !== ''
                                      && typeof parsed.apiKey === 'string'  && parsed.apiKey.trim()  !== '';
        } catch { vaultStatus.value.gelbooru = false; }
    } catch (e) { 
        console.warn("Vault check failed", e);
    }
};

/**
 * Goes from the result screen back to the welcome screen after a delay, once everything is done.
 * Does nothing while images are still queued or shown. It's called again whenever the UI loop
 * finishes or the queue shrinks, and cancelled when a new image starts.
 */
const armReturnToWelcome = () => {
    clearTimeout(returnToWelcomeTimer);

    if (appState.value !== 'result' || isUILoopRunning || uiQueue.length > 0 || queueCount.value > 0) return;

    returnToWelcomeTimer = window.setTimeout(() => {
        if (appState.value === 'result' && !isUILoopRunning && uiQueue.length === 0 && queueCount.value === 0) {
            appState.value = 'welcome';
        }
    }, RESULT_LINGER_MS);
};

/**
 * A dedicated asynchronous consumer loop for UI events.
 * Because the Rust backend sometimes processes images faster than the UI can display them,
 * this loop queues the incoming events and forces artificial delays (e.g., waiting at least
 * 600ms per image, or 2000ms on a success screen). This ensures the user actually sees 
 * the loading quotes and results before the app snaps back to the welcome screen.
 */
const runUILoop = async () => {
    isUILoopRunning = true;
    clearTimeout(returnToWelcomeTimer);

    while (uiQueue.length > 0) {
        // Processing already runs during the boot/update screen; just don't show it until that's done
        while (appState.value === 'updating') await sleep(250);

        const event = uiQueue.shift()!;

        if (event.type === 'proc') {
            // Pick a random loading quote based on weighted probabilities
            const totalWeight = sauceQuotes.reduce((sum, q) => sum + q.weight, 0);
            let random = Math.random() * totalWeight;
            let selectedQuote = sauceQuotes[0];

            for (const quote of sauceQuotes) {
                random -= quote.weight;
                if (random <= 0) {
                    selectedQuote = quote;
                    break;
                }
            }

            loadingMessage.value = selectedQuote.text;
            isRareMessage.value = selectedQuote.rare;

            appState.value = 'processing';
            activeImage.value = convertFileSrc(event.payload.path);
            lastProcStartTime = Date.now();

            const imageInfo = `${event.payload.width}×${event.payload.height}, ${event.payload.size_kb} KB`;
            if (imageInfoByFile.size > 200) imageInfoByFile.clear();
            imageInfoByFile.set(event.payload.filename, imageInfo);

            logger.debug(`Processing ${event.payload.filename}`, { details: toDetails([['Image', imageInfo]]) });
        }
        else if (event.type === 'succ') {
            // Force a minimum loading screen time of 600ms so the UI doesn't flash
            const elapsed = Date.now() - lastProcStartTime;
            if (elapsed < 600) await sleep(600 - elapsed);

            const p = event.payload;
            resultData.value = {
                name:    p.name,
                fandom:  p.fandom,
                artist:  p.artist,
                service: p.service,
                dest:    p.file_path,
                conf:    p.similarity,
                isOriginalWork: p.is_original_work
            };

            appState.value = 'result';

            const isOriginal = p.name === 'Original' || p.name === '0riginal';
            const hasArtist = p.artist && p.artist !== 'Unknown';
            const who = isOriginal
                ? `Original by ${hasArtist ? p.artist : 'Anon'}`
                : `${p.fandom} / ${p.name}`;
            const file = p.source_file || 'Image';

            logger.success(`${file} → ${who} (${p.similarity}%)`, {
                details: toDetails([
                    ['Service', p.service],
                    ['Match', `${p.similarity}%`],
                    ['Saved to', p.file_path],
                    ['Artist', hasArtist ? p.artist : null],
                    ['Image', imageInfoByFile.get(p.source_file)],
                ])
            });
            imageInfoByFile.delete(p.source_file);

            // Keep the success result on screen for 2 seconds before grabbing the next queue item
            await sleep(2000);
        }
        else if (event.type === 'fail') {
            const elapsed = Date.now() - lastProcStartTime;
            if (elapsed < 600) await sleep(600 - elapsed);

            // The backend sends { file, message, moved_to } (a plain string is accepted too)
            const failure = typeof event.payload === 'string' ? { message: event.payload } : (event.payload || {});
            const reason = failure.message || "An unknown processing error occurred";

            errorMessage.value = reason;
            appState.value = 'error';

            const subject = failure.file ? `${failure.file}: ` : '';
            const movedNote = failure.moved_to ? ` (moved to ${failure.moved_to})` : '';
            logger.error(`${subject}${reason}${movedNote}`, {
                details: toDetails([
                    ['Moved to', failure.moved_to],
                    ['Image', failure.file ? imageInfoByFile.get(failure.file) : null],
                ])
            });
            if (failure.file) imageInfoByFile.delete(failure.file);

            // Keep the error on screen for 3 seconds
            await sleep(3000);
            appState.value = 'welcome';
        }
    }

    isUILoopRunning = false;
    armReturnToWelcome();
};

/**
 * Subscribes the Vue frontend to the asynchronous events emitted by the Rust backend.
 * Routes processing states, warnings, and completions into the `uiQueue` to be handled
 * safely by the `runUILoop` consumer.
 */
export const initTauriListeners = async () => {
    if (isInitialized) return;
    isInitialized = true;

    logger.info('SauceBottle started.');

    await listen('queue-add', (ev: any) => {
        queueCount.value++;
        logger.debug(`Queued ${ev.payload || 'a file'} (${queueCount.value} in queue)`);
    });

    await listen('task-done', () => {
        if (queueCount.value > 0) queueCount.value--;
        syncOfflineQueue();
        armReturnToWelcome();
    });

    await listen('image-processing', (ev: any) => {
        uiQueue.push({ type: 'proc', payload: ev.payload });
        if (!isUILoopRunning) runUILoop();
    });

    await listen('warn', (ev: any) => {
        logger.warn(ev.payload);
    });

    await listen('info', (ev: any) => {
        logger.info(ev.payload);
    });

    await listen('success', (ev: any) => {
        uiQueue.push({ type: 'succ', payload: ev.payload });
        if (!isUILoopRunning) runUILoop();
    });

    await listen('failure', (ev: any) => {
        uiQueue.push({ type: 'fail', payload: ev.payload });
        if (!isUILoopRunning) runUILoop();
    });
};