This version adds an input folder setting, optional statistics, support for network drives and a much clearer Logs tab.

### Changelog

#### Improvements
* New `Input Folder` setting: choose which folder _SauceBottle_ watches for new images. Leave it empty to keep using the default `input` folder. The output folder can't be inside the input folder.
* New **Check for updates** button in the settings.
* After an update (and only then!), the window now always opens, even if `Start minimized` is turned on.
* New optional **Statistics** card on the start screen. Turn on `Collect statistics` in the settings to see images sorted, success rate, confidence, top character, fandom and artist, top service, storage, time per image, originals, downloads and time saved.
* The input and output folder can now be on a network drive (NAS). If one of them can't be reached, _SauceBottle_ pauses and tells you in the Logs tab, instead of searching every image over and over. It continues on its own once the drive is back. Saving settings or clicking "Open folder" no longer freezes the window while a drive is offline.
* The Logs tab is easier to read.
* Made the tag blacklist field bigger.
* Further UI improvements.

#### Bugfixes
* Fixed the last result staying on screen forever (showing "0 images in queue"). _SauceBottle_ now returns to the start screen a few seconds after the last image is sorted.
* Fixed "Open folder" not working for network drives.

### Installation

1. Download the appropriate installer for your operating system from the **Assets** section below (`.exe` for Windows, `.dmg` or `.app` for macOS, `.deb` or `AppImage` for Linux).
2. Run the installer and launch SauceBottle.