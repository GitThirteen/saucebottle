This version brings multiple bugfixes and new features to SauceBottle.

### Changelog

#### New Features
* Added a `Start minimized` setting which hides the app on startup / launch and places it immediately into the tray.
* _SauceBottle_ now supports multiple new image formats: `JFIF`, `ICO`, `TGA`, `AVIF`, `HEIC/HEIF`.

#### Improvements
* _SauceBottle_ now allows the path to be copied to the local clipboard by clicking the path box.
* Tips in the updating screen are now randomized.
* _SauceBottle_ now displays a warning if the IQDB servers are under high load.

#### Bugfixes
* Fixed a bug that caused top results from unsupported services to trigger an error state.
* The result folder path will now no longer escape its box.
* Fixed cloud and phone transfers to throw `Timed out waiting for file to finish writing to disk.` due to missing write access.
* Fixed the home menu reverting to the welcome screen if IQDB was faster at returning a result than SauceBottle displaying the image in the app.
* Starting with the next version, the patch notes will finally appear correctly if the auto-updater detects a new update.

#### Miscellaneous
* If you want to have a glimpse at future features and open issues / bugs, please take a look at the To-Do board on the _SauceBottle_ GitHub page! [[Take me there!]](https://github.com/users/GitThirteen/projects/1)

### Installation

1. Download the appropriate installer for your operating system from the **Assets** section below (`.exe` for Windows, `.dmg` or `.app` for macOS, `.deb` or `AppImage` for Linux).
2. Run the installer and launch SauceBottle.