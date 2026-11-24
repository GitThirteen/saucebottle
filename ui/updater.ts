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

import { invoke } from '@tauri-apps/api/core';
import { type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

import { appState, updateProgress, updateStatus } from './store';

/**
 * Downloads and installs an update, then restarts SauceBottle.
 * Shows the 'updating' screen with a progress bar while it runs.
 * Throws if the download or installation fails. The screen is reset first.
 *
 * @param update - The update found by the updater plugin's `check()`.
 */
export const installUpdate = async (update: Update) => {
  appState.value = 'updating';
  updateStatus.value = `Downloading v${update.version}...`;
  updateProgress.value = 0;

  try {
    // So the restarted app shows its window even with "start minimized" on
    try {
      await invoke('mark_update_restart', { version: update.version });
    } catch (e) {
      console.warn("Could not mark the update restart", e);
    }

    let downloaded = 0;
    let contentLength = 0;

    await update.downloadAndInstall((event) => {
      switch (event.event) {
        case 'Started':
          contentLength = event.data.contentLength || 0;
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
  } catch (e) {
    appState.value = 'welcome';
    throw e;
  }
};
