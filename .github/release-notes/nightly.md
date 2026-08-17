**This is a development build**, produced automatically from the latest commit on `main`. The
project is unfinished — see the status table in the README for what actually works today.

### Before it will do anything

The app ships with **no card data** and there is no in-app downloader yet, so a freshly
installed build opens on a screen telling you the catalog is missing. Building the data
currently needs a checkout of the repository:

```
cargo run --release -p build-artifacts
```

### Warnings you will see

Nothing here is code-signed, so Windows SmartScreen and macOS Gatekeeper will both object.

- **Windows** — "More info" then "Run anyway".
- **macOS** — right-click the app, then "Open".
- **Linux** — the AppImage needs `chmod +x` before it will run.

### Where your data lives

Everything stays on your device. The collection database is in the platform application data
directory under `dev.eraflo.magicoptimizer`; deleting that folder resets the app.

---

MagicOptimizer is unofficial Fan Content permitted under the Fan Content Policy. Not
approved/endorsed by Wizards. Portions of the materials used are property of Wizards of the
Coast. ©Wizards of the Coast LLC.
