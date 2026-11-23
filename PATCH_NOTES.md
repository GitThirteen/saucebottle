This version fixes a large number of bugs, most importantly images being cut off while they are sorted and SauceBottle sometimes not picking up your images after starting.

### Changelog

#### Improvements
* _SauceBottle_ now only runs once. Launching it again brings the already running window to the front instead of starting a second copy.
* Files that are moved or renamed into the `input` folder (e.g. by Explorer, or by a browser finishing a download) are now detected. Unfinished downloads (`.part`, `.crdownload`, ...) are ignored until they are done.
* While listening, the `input` folder is now re-checked every minute, so nothing gets left behind if the system misses a file.
* Text fields in the settings now only save once you press Enter or leave the field, and are highlighted until then. All other settings are saved shortly after you change them.
* Damaged or invalid settings are now handled gracefully: only the affected setting is reset, a damaged settings file is kept as `config.broken.json`, and the Logs tab tells you what happened.
* API keys are no longer shown in error messages, and dropping files or folders onto the app now reports the result in the Logs tab.

#### Bugfixes
* Fixed images being cut off (and duplicated in the `.invalid` folder) when they were picked up while still being copied. Files are now only processed once they are completely written and no longer in use by another program, and a failed move can no longer leave a partial copy behind.
* Fixed _SauceBottle_ sometimes acting as if it was paused after starting and only picking up images after pausing and resuming. Processing no longer waits for the update check, which now also gives up after 15 seconds.
* Fixed the queue getting stuck for good when IQDB or a booru site stopped responding. Requests now time out.
* Fixed images being sorted into the `Original` folder when the lookup failed (wrong credentials, deleted post, rate limit, failed tag lookup). They now go to the `.invalid` folder, with the reason in the Logs tab.
* Fixed manual mode (automatic processing turned off) never stopping after a successful run, and pressing Run with nothing to process.
* Fixed the batch downloader losing its Abort button when switching tabs, which also allowed a second download to be started in parallel.
* Fixed batch downloads failing for tags or credentials with special characters (like `&`, `#` or `+`), for start page `0`, and error pages being saved as image files.
* Fixed one invalid value in the settings file resetting all settings, and half-typed folder paths being applied while typing.
* Fixed sorting failing for folder names Windows cannot create (like `Con` or names ending with a dot).
* Fixed duplicates set to `Delete` being counted as handled when the file could not be deleted.
* Fixed false "file type mismatch" warnings for `.jfif` and `.heif` files.
* Fixed an invalid match threshold of `0%` resetting to `80%`.
* Fixed the original artwork result screen not working with a custom `Original Artwork Folder`.
* Fixed dropping a folder that contains SauceBottle's own folders (like `Pictures`) importing your whole sorted library again.
* Fixed the blacklist's typo detection not working for non-ASCII tags (e.g. Japanese).

### Installation

1. Download the appropriate installer for your operating system from the **Assets** section below (`.exe` for Windows, `.dmg` or `.app` for macOS, `.deb` or `AppImage` for Linux).
2. Run the installer and launch SauceBottle.